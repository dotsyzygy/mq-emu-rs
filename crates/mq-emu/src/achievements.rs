//! Achievements (`${Achievement}`): every achievement and category the
//! client knows, and your progress on them.
//!
//! The client learns of achievements from the server; on a server that sends
//! none, [`Achievements::is_ready`] stays false and the lists are empty.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mq_emu_sys::game::Api;
use mq_emu_sys::layouts::{
    Achievement as RawAchievement, AchievementCategory as RawCategory, AchievementComponent, AchievementManager, AchievementState as RawState,
    DynamicBitField_unsigned_short_short, SingleAchievementAndComponentsInfo,
};

use crate::game_fns::{MAX_ARRAY_LEN, game_array, text};
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::{from_cxstr, to_c};

/// `AchievementComponentType`: the objectives that unlock an achievement,
/// complete it, and count towards it indirectly.
const UNLOCK: usize = 0;
const COMPLETION: usize = 1;
const INDIRECT: usize = 2;

/// Element `index` of an `ArrayClass2`: bins of `1 << shift` elements.
///
/// # Safety
/// `bins` must be null or an `ArrayClass2`'s live bins, holding `len` elements.
unsafe fn binned<'a, T>(bins: *mut *mut T, len: i32, shift: i32, mask: i32, index: usize) -> Option<&'a T> {
    let len = usize::try_from(len).unwrap_or(0).min(MAX_ARRAY_LEN);
    if bins.is_null() || index >= len || !(0..32).contains(&shift) {
        return None;
    }
    let bin = unsafe { *bins.add(index >> shift) };
    if bin.is_null() {
        return None;
    }
    unsafe { bin.add(index & usize::try_from(mask).ok()?).as_ref() }
}

/// Whether bit `bit` of a `DynamicBitField<uint16_t, int16_t>` is set: the
/// first 16 bits are inline, the rest in `m_elements`.
fn bit_set(field: &DynamicBitField_unsigned_short_short, bit: usize) -> bool {
    if bit >= usize::from(field.m_numBits()) {
        return false;
    }
    let (element, offset) = (bit / 16, bit % 16);
    let word = if element == 0 {
        field.m_element()
    } else {
        let elements = field.m_elements();
        if elements.is_null() {
            return false;
        }
        unsafe { *elements.add(element - 1) }
    };
    word as u16 & (1 << offset) != 0
}

/// Where you are with an achievement (`${Achievement.State}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AchievementState {
    /// "COMPLETED".
    Completed,
    /// "OPEN": unlocked, not done yet.
    Open,
    /// "LOCKED": its unlock objectives aren't done.
    Locked,
    /// "HIDDEN": not shown to you.
    Hidden,
}

/// The achievement manager, for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct Achievements<'a> {
    raw: &'a AchievementManager,
    api: &'static Api,
}

impl<'a> Achievements<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a AchievementManager {
        self.raw
    }

    /// Whether the server has sent the achievements and your progress on
    /// them (`${Achievement.Ready}`).
    pub fn is_ready(&self) -> bool {
        self.raw.achievementClientReadOnlyDataSet() && self.raw.achievementClientStatesSet()
    }

    /// Every achievement (`${Achievement.AchievementByIndex[n]}`, from 1).
    pub fn iter(&self) -> impl Iterator<Item = Achievement<'a>> + 'a {
        let all = *self;
        (0..self.len()).filter_map(move |index| all.by_index(index))
    }

    /// How many achievements there are (`${Achievement.AchievementCount}`).
    pub fn len(&self) -> usize {
        usize::try_from(self.raw.achievements().m_length()).unwrap_or(0).min(MAX_ARRAY_LEN)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Achievement `index` (0-based).
    pub fn by_index(&self, index: usize) -> Option<Achievement<'a>> {
        let list = self.raw.achievements();
        let raw = unsafe { binned(list.m_array(), list.m_length(), list.m_binShift(), list.m_slotMask(), index) }?;
        Some(Achievement { all: *self, raw, index })
    }

    /// The achievement with this ID (`${Achievement[id]}`).
    pub fn get(&self, id: i32) -> Option<Achievement<'a>> {
        self.iter().find(|achievement| achievement.id() == id)
    }

    /// The achievement with this name, ignoring case (`${Achievement[name]}`).
    pub fn by_name(&self, name: &str) -> Option<Achievement<'a>> {
        self.iter().find(|achievement| achievement.name().eq_ignore_ascii_case(name))
    }

    /// Every category, top-level and nested (`${Achievement.CategoryByIndex[n]}`).
    pub fn categories(&self) -> impl Iterator<Item = AchievementCategory<'a>> + 'a {
        let all = *self;
        (0..self.category_count()).filter_map(move |index| all.category_by_index(index))
    }

    /// `${Achievement.CategoryCount}`.
    pub fn category_count(&self) -> usize {
        usize::try_from(self.raw.categories().m_length()).unwrap_or(0).min(MAX_ARRAY_LEN)
    }

    /// Category `index` (0-based).
    pub fn category_by_index(&self, index: usize) -> Option<AchievementCategory<'a>> {
        let list = self.raw.categories();
        let raw = unsafe { binned(list.m_array(), list.m_length(), list.m_binShift(), list.m_slotMask(), index) }?;
        Some(AchievementCategory { all: *self, raw, index })
    }

    /// The category with this ID (`${Achievement.Category[id]}`).
    pub fn category(&self, id: i32) -> Option<AchievementCategory<'a>> {
        self.categories().find(|category| category.id() == id)
    }

    /// The category with this name, ignoring case (`${Achievement.Category[name]}`).
    pub fn category_by_name(&self, name: &str) -> Option<AchievementCategory<'a>> {
        self.categories().find(|category| category.name().eq_ignore_ascii_case(name))
    }

    /// Your achievement points (`${Achievement.Points}`).
    pub fn points(&self) -> u32 {
        self.raw.completedAchievementScore()
    }

    /// `${Achievement.CompletedAchievements}`.
    pub fn completed_count(&self) -> u32 {
        self.raw.completedAchievementCount()
    }

    /// Completed, open and locked achievements (`${Achievement.TotalAchievements}`).
    pub fn total_count(&self) -> u32 {
        self.raw.completedAchievementCount() + self.raw.lockedAchievemmentCount() + self.raw.openAchievementCount()
    }

    /// Your progress on achievement `index`.
    fn progress(&self, index: usize) -> Option<&'a SingleAchievementAndComponentsInfo> {
        let list = self.raw.achievementClientInfoArray();
        unsafe { game_array(list.m_array(), list.m_length()) }.get(index)
    }
}

impl fmt::Debug for Achievements<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Achievements").field("ready", &self.is_ready()).field("len", &self.len()).finish()
    }
}

/// One achievement.
#[derive(Clone, Copy)]
pub struct Achievement<'a> {
    all: Achievements<'a>,
    raw: &'a RawAchievement,
    index: usize,
}

impl<'a> Achievement<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a RawAchievement {
        self.raw
    }

    pub fn id(&self) -> i32 {
        self.raw.id()
    }

    /// Its place in the manager's list, 0-based (`${Achievement.Index}` counts from 1).
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn name(&self) -> String {
        from_cxstr(self.raw.name())
    }

    pub fn description(&self) -> String {
        from_cxstr(self.raw.description())
    }

    /// What completing it is worth.
    pub fn points(&self) -> i32 {
        self.raw.points()
    }

    /// `${Achievement.IconID}`.
    pub fn icon_id(&self) -> i32 {
        self.raw.iconId()
    }

    /// Where you are with it; hidden when the game has no progress for it.
    pub fn state(&self) -> AchievementState {
        match self.all.progress(self.index).map(SingleAchievementAndComponentsInfo::achievementState) {
            Some(RawState::AchievementComplete) => AchievementState::Completed,
            Some(RawState::AchievementOpen) => AchievementState::Open,
            Some(RawState::AchievementLocked) => AchievementState::Locked,
            _ => AchievementState::Hidden,
        }
    }

    /// When you completed it (`${Achievement.CompletedTime}`).
    pub fn completed_at(&self) -> Option<SystemTime> {
        let seconds = self.all.progress(self.index)?.completionTimestamp() as u32;
        (seconds != 0).then(|| UNIX_EPOCH + Duration::from_secs(u64::from(seconds)))
    }

    /// The objectives that count now (`${Achievement.ObjectiveByIndex[n]}`):
    /// the ones that unlock it while it's locked, otherwise the ones that
    /// complete it.
    pub fn objectives(&self) -> impl Iterator<Item = AchievementObjective<'a>> + 'a {
        let kinds: &[usize] = if self.state() == AchievementState::Locked { &[UNLOCK] } else { &[COMPLETION, INDIRECT] };
        let achievement = *self;
        kinds.iter().flat_map(move |&kind| achievement.components(kind))
    }

    fn components(self, kind: usize) -> impl Iterator<Item = AchievementObjective<'a>> + 'a {
        let list = self.raw.componentsByType(kind);
        let progress = self.all.progress(self.index);
        unsafe { game_array(list.m_array(), list.m_length()) }
            .iter()
            .enumerate()
            .map(move |(position, raw)| AchievementObjective { raw, kind, position, progress })
    }

    /// The objective with this ID (`${Achievement.Objective[id]}`), whether it
    /// counts now or not.
    pub fn objective(&self, id: i32) -> Option<AchievementObjective<'a>> {
        [COMPLETION, INDIRECT, UNLOCK].into_iter().flat_map(|kind| self.components(kind)).find(|objective| objective.id() == id)
    }

    /// A chat link to it as `player` has it (`${Achievement.Link[player]}`).
    pub fn link(&self, player: &str) -> String {
        let player = to_c(player);
        let raw = std::ptr::from_ref(self.raw);
        let api = self.all.api;
        text(|out, cap| unsafe { (api.mqemu_achievement_link)(raw, player.as_ptr(), out, cap) }).unwrap_or_default()
    }
}

impl fmt::Debug for Achievement<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Achievement").field("id", &self.id()).field("name", &self.name()).field("state", &self.state()).finish()
    }
}

/// One objective of an achievement.
#[derive(Clone, Copy)]
pub struct AchievementObjective<'a> {
    raw: &'a AchievementComponent,
    kind: usize,
    /// Its place among the achievement's objectives of its kind.
    position: usize,
    progress: Option<&'a SingleAchievementAndComponentsInfo>,
}

impl<'a> AchievementObjective<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a AchievementComponent {
        self.raw
    }

    pub fn id(&self) -> i32 {
        self.raw.id()
    }

    /// Where the achievement window shows it (`${...Objective.Index}`).
    pub fn sequence(&self) -> i32 {
        self.raw.sequenceNum()
    }

    pub fn description(&self) -> String {
        from_cxstr(self.raw.description())
    }

    /// `${...Objective.Count}`.
    pub fn count(&self) -> i32 {
        self.raw.count()
    }

    /// `${...Objective.RequiredCount}`.
    pub fn required_count(&self) -> i32 {
        self.raw.requiredCount()
    }

    /// Whether you've done it (`${...Objective.Completed}`).
    pub fn is_completed(&self) -> bool {
        let Some(progress) = self.progress else { return false };
        let field = match self.kind {
            COMPLETION => progress.completionComponentStatusBitField(),
            INDIRECT => progress.indirectComponentStatusBitField(),
            _ => progress.unlockedComponentStatusBitField(),
        };
        bit_set(field, self.position)
    }

    /// Whether it unlocks the achievement, rather than counting towards
    /// completing it.
    pub fn unlocks(&self) -> bool {
        self.kind == UNLOCK
    }
}

impl fmt::Debug for AchievementObjective<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AchievementObjective").field("id", &self.id()).field("description", &self.description()).field("completed", &self.is_completed()).finish()
    }
}

/// A category of achievements, as the achievement window groups them.
#[derive(Clone, Copy)]
pub struct AchievementCategory<'a> {
    all: Achievements<'a>,
    raw: &'a RawCategory,
    index: usize,
}

impl<'a> AchievementCategory<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a RawCategory {
        self.raw
    }

    pub fn id(&self) -> i32 {
        self.raw.id()
    }

    /// Its place in the manager's list, 0-based (`${...Category.Index}` counts from 1).
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn name(&self) -> String {
        from_cxstr(self.raw.name())
    }

    pub fn description(&self) -> String {
        from_cxstr(self.raw.description())
    }

    /// The name of its image (`${...Category.ImageTextureName}`).
    pub fn image_name(&self) -> String {
        from_cxstr(self.raw.bitmapId())
    }

    /// The category it's in, if it isn't a top-level one.
    pub fn parent(&self) -> Option<AchievementCategory<'a>> {
        let parent = self.raw.parentId();
        if parent > 0 { self.all.category(parent) } else { None }
    }

    /// The categories in it (`${...Category.CategoryByIndex[n]}`).
    pub fn children(&self) -> impl Iterator<Item = AchievementCategory<'a>> + 'a {
        let list = self.raw.childCategories();
        let all = self.all;
        unsafe { game_array(list.m_array(), list.m_length()) }.iter().filter_map(move |&id| all.category(id))
    }

    /// The achievements in it (`${...Category.AchievementByIndex[n]}`).
    pub fn achievements(&self) -> impl Iterator<Item = Achievement<'a>> + 'a {
        let list = self.raw.achievements();
        let all = self.all;
        unsafe { game_array(list.m_array(), list.m_length()) }.iter().filter_map(move |info| all.get(info.achievementId()))
    }

    /// Your points in it (`${...Category.Points}`).
    pub fn points(&self) -> i32 {
        self.raw.completedAchievementScore()
    }

    /// `${...Category.CompletedAchievements}`.
    pub fn completed_count(&self) -> i32 {
        self.raw.completedAchievementCount()
    }

    /// `${...Category.TotalAchievements}`.
    pub fn total_count(&self) -> i32 {
        self.raw.completedAchievementCount() + self.raw.openAchievementCount() + self.raw.lockedAchievementCount()
    }
}

impl fmt::Debug for AchievementCategory<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AchievementCategory").field("id", &self.id()).field("name", &self.name()).finish()
    }
}

impl<P: Plugin> Mq<P> {
    /// The achievements, in game. Check [`Achievements::is_ready`]: on a
    /// server that sends no achievements it stays false.
    pub fn achievements(&self) -> Option<Achievements<'_>> {
        self.local_pc()?;
        let api = self.host().game_fns;
        let raw = unsafe { (api.mqemu_achievement_manager)().as_ref() }?;
        Some(Achievements { raw, api })
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::layouts::{AchievementInfo, CXStr};

    use super::*;
    use crate::game_fns::fake;
    use crate::items::tests::Fake;
    use crate::text::tests::cstrrep;

    /// A game record of all zeros.
    fn zeroed<T>() -> T {
        unsafe { std::mem::zeroed() }
    }

    /// A manager with `achievements` (copied) in bins of 2, so the second bin
    /// is used, and `progress` alongside.
    struct FakeManager {
        manager: Fake<AchievementManager>,
        _bins: Vec<Vec<RawAchievement>>,
        _bin_pointers: Vec<*mut RawAchievement>,
    }

    fn set_text(text: &CXStr, value: &str, keep: &mut Vec<Vec<u64>>) {
        let rep = cstrrep(0, value.len() as u32, value.as_bytes());
        text.set_m_data(rep.as_ptr().cast_mut().cast());
        keep.push(rep);
    }

    fn manager(achievements: &[Fake<RawAchievement>], progress: &mut [SingleAchievementAndComponentsInfo]) -> FakeManager {
        let manager = Fake::<AchievementManager>::new();
        let mut bins: Vec<Vec<RawAchievement>> = achievements
            .chunks(2)
            .map(|chunk| chunk.iter().map(|a| unsafe { std::ptr::read(a.ptr()) }).collect())
            .collect();
        let mut bin_pointers: Vec<*mut RawAchievement> = bins.iter_mut().map(|bin| bin.as_mut_ptr()).collect();
        let list = manager.get().achievements();
        list.set_m_array(bin_pointers.as_mut_ptr());
        list.set_m_length(achievements.len() as i32);
        list.set_m_binShift(1);
        list.set_m_slotMask(1);
        let info = manager.get().achievementClientInfoArray();
        info.set_m_array(progress.as_mut_ptr());
        info.set_m_length(progress.len() as i32);
        FakeManager { manager, _bins: bins, _bin_pointers: bin_pointers }
    }

    fn achievements(fake: &FakeManager) -> Achievements<'_> {
        Achievements { raw: fake.manager.get(), api: &fake::API }
    }

    #[test]
    fn achievements_are_read_across_bins() {
        let mut keep = Vec::new();
        let raw: Vec<_> = (0..3).map(|_| Fake::<RawAchievement>::new()).collect();
        for (i, (achievement, name)) in raw.iter().zip(["First", "Second", "Third"]).enumerate() {
            achievement.get().set_id(100 + i as i32);
            set_text(achievement.get().name(), name, &mut keep);
        }
        let mut states: [SingleAchievementAndComponentsInfo; 2] = [zeroed(), zeroed()];
        states[0].set_achievementState(RawState::AchievementComplete);
        states[0].set_completionTimestamp(1_700_000_000);
        states[1].set_achievementState(RawState::AchievementLocked);
        let fake = manager(&raw, &mut states);
        let all = achievements(&fake);
        let found: Vec<_> = all.iter().map(|a| (a.id(), a.name(), a.state())).collect();
        assert_eq!(found, [
            (100, "First".into(), AchievementState::Completed),
            (101, "Second".into(), AchievementState::Locked),
            (102, "Third".into(), AchievementState::Hidden),
        ]);
        assert_eq!(all.by_name("third").map(|a| a.index()), Some(2));
        assert_eq!(all.get(101).map(|a| a.name()), Some("Second".into()));
        assert_eq!(all.get(1).map(|a| a.id()), None);
        assert_eq!(all.get(100).and_then(|a| a.completed_at()), Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000)));
        assert_eq!(all.get(101).and_then(|a| a.completed_at()), None);
        assert!(!all.is_ready(), "nothing has arrived");
        assert_eq!(all.get(100).map(|a| a.link("Tester")), Some("achievement 100 of Tester".into()));
    }

    #[test]
    fn an_empty_manager_lists_nothing() {
        let fake = manager(&[], &mut []);
        let all = achievements(&fake);
        assert!(all.is_empty());
        assert_eq!(all.iter().count(), 0);
        assert_eq!(all.categories().count(), 0);
        assert!(all.by_index(0).is_none());
    }

    #[test]
    fn objectives_count_unlocks_only_while_locked() {
        let raw = [Fake::<RawAchievement>::new()];
        let mut components: Vec<Vec<AchievementComponent>> = (0..3)
            .map(|kind| {
                (0..2)
                    .map(|i| {
                        let component: AchievementComponent = zeroed();
                        component.set_id(kind * 10 + i);
                        component
                    })
                    .collect()
            })
            .collect();
        for (kind, list) in components.iter_mut().enumerate() {
            raw[0].get().componentsByType(kind).set_m_array(list.as_mut_ptr());
            raw[0].get().componentsByType(kind).set_m_length(2);
        }
        let mut states: [SingleAchievementAndComponentsInfo; 1] = [zeroed()];
        states[0].set_achievementState(RawState::AchievementOpen);
        // The second completion objective is done.
        states[0].completionComponentStatusBitField().set_m_numBits(2);
        states[0].completionComponentStatusBitField().set_m_element(0b10);
        let fake = manager(&raw, &mut states);
        let achievement = achievements(&fake).by_index(0).unwrap();
        let open: Vec<_> = achievement.objectives().map(|o| (o.id(), o.is_completed())).collect();
        assert_eq!(open, [(10, false), (11, true), (20, false), (21, false)]);
        assert_eq!(achievement.objective(1).map(|o| o.unlocks()), Some(true), "found while it doesn't count");
        states[0].set_achievementState(RawState::AchievementLocked);
        let locked: Vec<_> = achievement.objectives().map(|o| o.id()).collect();
        assert_eq!(locked, [0, 1]);
    }

    #[test]
    fn bits_past_the_first_sixteen_come_from_the_elements() {
        let field = Fake::<DynamicBitField_unsigned_short_short>::new();
        let mut elements = [0b100i16];
        field.get().set_m_numBits(40);
        field.get().set_m_element(1);
        field.get().set_m_elements(elements.as_mut_ptr());
        assert!(bit_set(field.get(), 0));
        assert!(!bit_set(field.get(), 1));
        assert!(bit_set(field.get(), 18));
        assert!(!bit_set(field.get(), 50), "past the end");
        field.get().set_m_elements(std::ptr::null_mut());
        assert!(!bit_set(field.get(), 18));
    }

    #[test]
    fn categories_find_their_achievements_children_and_parent() {
        let mut keep = Vec::new();
        let raw = [Fake::<RawAchievement>::new()];
        raw[0].get().set_id(7);
        let fake = manager(&raw, &mut []);
        let mut categories: Vec<RawCategory> = (0..2).map(|_| zeroed()).collect();
        categories[0].set_id(1);
        set_text(categories[0].name(), "Exploration", &mut keep);
        categories[1].set_id(2);
        categories[1].set_parentId(1);
        let mut children = [2];
        categories[0].childCategories().set_m_array(children.as_mut_ptr());
        categories[0].childCategories().set_m_length(1);
        let mut infos: [AchievementInfo; 1] = [zeroed()];
        infos[0].set_achievementId(7);
        categories[1].achievements().set_m_array(infos.as_mut_ptr());
        categories[1].achievements().set_m_length(1);
        let mut bins = [categories.as_mut_ptr()];
        let list = fake.manager.get().categories();
        list.set_m_array(bins.as_mut_ptr());
        list.set_m_length(2);
        list.set_m_binShift(5);
        list.set_m_slotMask(31);
        let all = achievements(&fake);
        let top = all.category_by_name("exploration").unwrap();
        assert_eq!(top.children().map(|c| c.id()).collect::<Vec<_>>(), [2]);
        let child = all.category(2).unwrap();
        assert_eq!(child.parent().map(|c| c.id()), Some(1));
        assert!(top.parent().is_none());
        assert_eq!(child.achievements().map(|a| a.id()).collect::<Vec<_>>(), [7]);
    }
}
