//! Your EverQuest tasks, as the task window lists them (`${Task}`).
//!
//! The game calls them tasks; they're [`Quest`]s here because
//! [`Task`](crate::Task) is the plugin's own async work.

use std::fmt;
use std::time::Duration;

use mq_emu_sys::constants::{MAX_QUEST_ENTRIES, MAX_SHARED_TASK_ENTRIES, MAX_TASK_ELEMENTS};
use mq_emu_sys::game::Api;
use mq_emu_sys::layouts::{CTaskElement, CTaskEntry, CTaskManager, PCTaskStatus, SharedTaskPlayerInfo, TaskSystemType, eSharedTaskPlayerRole};

use crate::game_fns::text;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::{from_c_array, from_cxstr};
use crate::zone::ZoneId;

/// The most shared task members read, so a damaged list can't loop forever.
const MAX_MEMBERS: usize = 256;

/// Whether a task is yours alone or shared with others (`${Task.Type}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuestKind {
    /// "Quest".
    Solo,
    /// "Shared".
    Shared,
}

/// What an objective asks for (`${Task[n].Objective[m].Type}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectiveKind {
    None,
    Deliver,
    Kill,
    Loot,
    Hail,
    Explore,
    Tradeskill,
    Fishing,
    Foraging,
    Cast,
    UseSkill,
    DZSwitch,
    DestroyObject,
    Collect,
    Dialogue,
    /// A value the game has no name for.
    Other(i32),
}

impl ObjectiveKind {
    fn from_raw(value: i32) -> ObjectiveKind {
        use ObjectiveKind::*;
        match value {
            0 => None,
            1 => Deliver,
            2 => Kill,
            3 => Loot,
            4 => Hail,
            5 => Explore,
            6 => Tradeskill,
            7 => Fishing,
            8 => Foraging,
            9 => Cast,
            10 => UseSkill,
            11 => DZSwitch,
            12 => DestroyObject,
            13 => Collect,
            14 => Dialogue,
            other => Other(other),
        }
    }

    /// The name MacroQuest gives it: "Kill", "UseSkill", ...; "Unknown" for
    /// -1 and "NULL" for other values, as `${Task[n].Objective[m].Type}`.
    pub fn name(self) -> &'static str {
        use ObjectiveKind::*;
        match self {
            None => "None",
            Deliver => "Deliver",
            Kill => "Kill",
            Loot => "Loot",
            Hail => "Hail",
            Explore => "Explore",
            Tradeskill => "Tradeskill",
            Fishing => "Fishing",
            Foraging => "Foraging",
            Cast => "Cast",
            UseSkill => "UseSkill",
            DZSwitch => "DZSwitch",
            DestroyObject => "DestroyObject",
            Collect => "Collect",
            Dialogue => "Dialogue",
            Other(-1) => "Unknown",
            Other(_) => "NULL",
        }
    }
}

/// One of your tasks, for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct Quest<'a> {
    raw: &'a CTaskEntry,
    members: *mut SharedTaskPlayerInfo,
    api: &'static Api,
}

impl<'a> Quest<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a CTaskEntry {
        self.raw
    }

    /// What the game identifies the task by (`${Task.ID}`).
    pub fn id(&self) -> i32 {
        self.raw.TaskID()
    }

    pub fn title(&self) -> String {
        from_c_array(&self.raw.TaskTitle())
    }

    pub fn kind(&self) -> QuestKind {
        if self.raw.TaskSystem() == TaskSystemType::cTaskSystemTypeSharedQuest { QuestKind::Shared } else { QuestKind::Solo }
    }

    fn status(&self) -> Option<&'a PCTaskStatus> {
        let raw = std::ptr::from_ref(self.raw).cast_mut();
        unsafe { (self.api.mqemu_task_status)(raw).as_ref() }
    }

    /// The task's time limit, if it has one.
    pub fn duration(&self) -> Option<Duration> {
        let seconds = u64::try_from(self.raw.DurationSeconds()).ok().filter(|s| *s > 0)?;
        Some(Duration::from_secs(seconds))
    }

    /// The time left before it expires (`${Task.Timer}`), if it has a time
    /// limit; zero once it's up.
    pub fn time_left(&self) -> Option<Duration> {
        let duration = self.duration()?;
        let started = i64::from(self.status()?.MovingStartTime());
        let now = i64::from(unsafe { (self.api.mqemu_fast_time)() });
        let left = started + duration.as_secs() as i64 - now;
        Some(Duration::from_secs(u64::try_from(left).unwrap_or(0)))
    }

    /// Its objectives (`${Task.Objective[n]}`), in order.
    pub fn objectives(&self) -> impl Iterator<Item = QuestObjective<'a>> + 'a {
        let quest = *self;
        let status = self.status();
        (0..MAX_TASK_ELEMENTS).filter_map(move |index| quest.objective_at(index, status))
    }

    /// Objective `index` (0-based; `${Task.Objective[index+1]}`).
    pub fn objective(&self, index: usize) -> Option<QuestObjective<'a>> {
        self.objective_at(index, self.status())
    }

    fn objective_at(&self, index: usize, status: Option<&'a PCTaskStatus>) -> Option<QuestObjective<'a>> {
        if index >= MAX_TASK_ELEMENTS {
            return None;
        }
        let raw = self.raw.Elements(index);
        (raw.Type().0 > 0).then_some(QuestObjective { raw, index, status, api: self.api })
    }

    /// The first objective that isn't done and isn't optional
    /// (`${Task.Step}`): what to do next.
    pub fn step(&self) -> Option<QuestObjective<'a>> {
        self.objectives().find(|objective| !objective.is_optional() && objective.current_count().is_some_and(|count| count < objective.required_count()))
    }

    /// Who shares the task, in the order the game lists them (empty for a
    /// solo task). MacroQuest's `${Task.Member}` gives the shared task's
    /// members for any task.
    pub fn members(&self) -> impl Iterator<Item = QuestMember<'a>> + 'a {
        let mut next = if self.kind() == QuestKind::Shared { self.members } else { std::ptr::null_mut() };
        std::iter::from_fn(move || {
            let raw = unsafe { next.as_ref::<'a>() }?;
            next = raw.pNext();
            Some(QuestMember { raw })
        })
        .take(MAX_MEMBERS)
    }

    /// The member leading it (`${Task.Leader}`).
    pub fn leader(&self) -> Option<QuestMember<'a>> {
        self.members().find(QuestMember::is_leader)
    }
}

impl fmt::Debug for Quest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Quest").field("id", &self.id()).field("title", &self.title()).field("kind", &self.kind()).finish()
    }
}

/// One objective of a task.
#[derive(Clone, Copy)]
pub struct QuestObjective<'a> {
    raw: &'a CTaskElement,
    index: usize,
    status: Option<&'a PCTaskStatus>,
    api: &'static Api,
}

impl<'a> QuestObjective<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a CTaskElement {
        self.raw
    }

    /// Which of the task's objectives it is, 0-based (`${...Objective.Index}`
    /// counts from 1).
    pub fn index(&self) -> usize {
        self.index
    }

    /// What it asks, as the task window says it (`${...Objective.Instruction}`).
    pub fn text(&self) -> String {
        let raw = std::ptr::from_ref(self.raw);
        text(|out, cap| unsafe { (self.api.mqemu_task_objective_text)(raw, out, cap) }).unwrap_or_default()
    }

    pub fn kind(&self) -> ObjectiveKind {
        ObjectiveKind::from_raw(self.raw.Type().0)
    }

    /// How many it takes (`${...Objective.RequiredCount}`).
    pub fn required_count(&self) -> i32 {
        self.raw.RequiredCount()
    }

    /// How many you have so far (`${...Objective.CurrentCount}`), if the game
    /// has your progress on the task.
    pub fn current_count(&self) -> Option<i32> {
        self.status.map(|status| status.CurrentCounts_at(self.index))
    }

    /// Whether it's done (`${...Objective.Status}` is "Done").
    pub fn is_done(&self) -> bool {
        self.current_count() == Some(self.required_count())
    }

    pub fn is_optional(&self) -> bool {
        self.raw.bOptional()
    }

    /// The zone it's done in (`${...Objective.Zone}`); `None` for any zone.
    pub fn zone_id(&self) -> Option<ZoneId> {
        let zones = from_c_array(&self.raw.ZoneID());
        let digits = zones.trim_start().bytes().take_while(u8::is_ascii_digit).count();
        let id = zones.trim_start()[..digits].parse::<u32>().unwrap_or(0) & 0x7FFF;
        (id != 0).then_some(ZoneId(id as u16))
    }

    /// Who or what it's about (a creature to kill, an NPC to hail), if anyone.
    pub fn target_name(&self) -> String {
        from_c_array(&self.raw.TargetName())
    }

    /// The items it asks for, as the game lists them (`${...Objective.RequiredItem}`).
    pub fn required_item(&self) -> String {
        from_cxstr(self.raw.ItemNameList())
    }

    /// The skills it asks for, by ID (`${...Objective.RequiredSkill}`).
    pub fn required_skill(&self) -> String {
        from_cxstr(self.raw.SkillIDList())
    }

    /// The spells it asks for, by ID (`${...Objective.RequiredSpell}`).
    pub fn required_spell(&self) -> String {
        from_cxstr(self.raw.SpellIDList())
    }

    /// The switch it asks you to click (`${...Objective.DZSwitchID}`).
    pub fn dz_switch_id(&self) -> i32 {
        self.raw.DZSwitchID()
    }
}

impl fmt::Debug for QuestObjective<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuestObjective")
            .field("index", &self.index)
            .field("kind", &self.kind())
            .field("count", &self.current_count())
            .field("required", &self.required_count())
            .finish()
    }
}

/// Someone sharing your shared task.
#[derive(Clone, Copy)]
pub struct QuestMember<'a> {
    raw: &'a SharedTaskPlayerInfo,
}

impl<'a> QuestMember<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a SharedTaskPlayerInfo {
        self.raw
    }

    pub fn name(&self) -> String {
        from_c_array(&self.raw.Name())
    }

    pub fn is_leader(&self) -> bool {
        self.raw.Role() == eSharedTaskPlayerRole::eSharedTaskRoleLeader
    }
}

impl fmt::Debug for QuestMember<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuestMember").field("name", &self.name()).field("leader", &self.is_leader()).finish()
    }
}

/// Your tasks: the shared one first, then your solo ones, as `${Task[n]}`
/// counts them.
fn quests<'a>(manager: &'a CTaskManager, members: *mut SharedTaskPlayerInfo, api: &'static Api) -> impl Iterator<Item = Quest<'a>> + 'a {
    let shared = (0..MAX_SHARED_TASK_ENTRIES).map(|i| manager.SharedTaskEntries(i));
    let solo = (0..MAX_QUEST_ENTRIES).map(|i| manager.QuestEntries(i));
    shared.chain(solo).filter(|raw| raw.TaskID() != 0).map(move |raw| Quest { raw, members, api })
}

impl<P: Plugin> Mq<P> {
    /// Your tasks, as the task window lists them: a shared task first, then
    /// solo ones (`${Task[n]}` counts them in this order, from 1).
    pub fn quests(&self) -> impl Iterator<Item = Quest<'_>> + '_ {
        let manager = unsafe { (self.host().task_manager)().as_ref() };
        let members = (self.host().task_members)();
        manager.into_iter().flat_map(move |manager| quests(manager, members, self.host().game_fns))
    }

    /// Your task with this title, ignoring case (`${Task[=title]}`).
    pub fn quest(&self, title: &str) -> Option<Quest<'_>> {
        self.quests().find(|quest| quest.title().eq_ignore_ascii_case(title))
    }

    /// Your task with this ID.
    pub fn quest_by_id(&self, id: i32) -> Option<Quest<'_>> {
        self.quests().find(|quest| quest.id() == id)
    }

    /// The task window's row for a task (`${Task.WindowIndex}` is it plus
    /// one), if the window lists it.
    pub fn quest_window_row(&self, quest: &Quest<'_>) -> Option<usize> {
        let title = quest.title();
        self.task_list()?.find(2, |cell| cell.eq_ignore_ascii_case(&title))
    }

    /// Selects a task in the task window, as `${Task.Select}` does. False if
    /// the window doesn't list it.
    pub fn select_quest(&self, quest: &Quest<'_>) -> bool {
        let Some(list) = self.task_list() else { return false };
        let Some(row) = self.quest_window_row(quest) else { return false };
        list.select(row)
    }

    fn task_list(&self) -> Option<crate::ListWindow<'_>> {
        self.window("TaskWnd")?.child("TASK_TaskList")?.as_list()
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::layouts::TaskType;

    use super::*;
    use crate::game_fns::fake;
    use crate::items::tests::{Fake, set_name};

    /// A task manager with a solo quest in `QuestEntries[slot]`.
    fn add_quest<'a>(manager: &'a CTaskManager, slot: usize, id: i32, title: &str) -> &'a CTaskEntry {
        let raw = manager.QuestEntries(slot);
        raw.set_TaskID(id);
        raw.set_TaskTitle(set_name(title));
        raw.set_TaskSystem(TaskSystemType::cTaskSystemTypeSoloQuest);
        raw
    }

    fn objective(raw: &CTaskEntry, index: usize, kind: i32, required: i32) -> &CTaskElement {
        let element = raw.Elements(index);
        element.set_Type(TaskType(kind));
        element.set_RequiredCount(required);
        element
    }

    fn collect<'a>(manager: &'a CTaskManager, members: *mut SharedTaskPlayerInfo) -> Vec<Quest<'a>> {
        quests(manager, members, &fake::API).collect()
    }

    #[test]
    fn quests_list_the_shared_task_first_and_skip_empty_slots() {
        let fake = Fake::<CTaskManager>::new();
        let manager = fake.get();
        add_quest(manager, 0, 10, "Solo One");
        add_quest(manager, 2, 12, "Solo Two");
        let shared = manager.SharedTaskEntries(0);
        shared.set_TaskID(20);
        shared.set_TaskTitle(set_name("Together"));
        shared.set_TaskSystem(TaskSystemType::cTaskSystemTypeSharedQuest);
        let all = collect(manager, std::ptr::null_mut());
        let found: Vec<_> = all.iter().map(|q| (q.id(), q.title(), q.kind())).collect();
        assert_eq!(found, [
            (20, "Together".into(), QuestKind::Shared),
            (10, "Solo One".into(), QuestKind::Solo),
            (12, "Solo Two".into(), QuestKind::Solo),
        ]);
    }

    #[test]
    fn objectives_read_progress_from_the_task_status() {
        let fake = Fake::<CTaskManager>::new();
        let raw = add_quest(fake.get(), 0, 10, "Rats");
        let kill = objective(raw, 0, 2, 5);
        objective(raw, 1, 1, 1).set_bOptional(true);
        objective(raw, 2, 5, 1).set_ZoneID(set_name("33;34"));
        let status = Fake::<PCTaskStatus>::new();
        status.get().set_CurrentCounts_at(0, 5);
        fake::with(|game| {
            game.task_statuses.insert(std::ptr::from_ref(raw) as usize, status.ptr());
            game.objective_texts.insert(std::ptr::from_ref(kill) as usize, "Kill 5 rats".into());
        });
        let quest = collect(fake.get(), std::ptr::null_mut())[0];
        let objectives: Vec<_> = quest.objectives().collect();
        assert_eq!(objectives.len(), 3);
        assert_eq!((objectives[0].text(), objectives[0].kind(), objectives[0].is_done()), ("Kill 5 rats".into(), ObjectiveKind::Kill, true));
        assert_eq!((objectives[1].current_count(), objectives[1].is_optional()), (Some(0), true));
        assert_eq!(objectives[2].zone_id(), Some(ZoneId(33)));
        assert_eq!(objectives[0].zone_id(), None, "no zone is any zone");
        // The kill is done and the delivery optional: the explore is next.
        assert_eq!(quest.step().map(|o| o.index()), Some(2));
        assert_eq!(quest.objective(3).map(|o| o.index()), None);
        assert!(quest.objective(MAX_TASK_ELEMENTS).is_none());
    }

    #[test]
    fn without_progress_counts_are_unknown_and_nothing_is_done() {
        let fake = Fake::<CTaskManager>::new();
        let raw = add_quest(fake.get(), 0, 10, "Rats");
        objective(raw, 0, 2, 5);
        let quest = collect(fake.get(), std::ptr::null_mut())[0];
        let first = quest.objective(0).unwrap();
        assert_eq!((first.current_count(), first.is_done()), (None, false));
        assert!(quest.step().is_none());
        assert_eq!(quest.time_left(), None);
    }

    #[test]
    fn time_left_counts_down_from_the_moving_start() {
        let fake = Fake::<CTaskManager>::new();
        let raw = add_quest(fake.get(), 0, 10, "Timed");
        raw.set_DurationSeconds(600);
        let status = Fake::<PCTaskStatus>::new();
        status.get().set_MovingStartTime(1000);
        fake::with(|game| {
            game.task_statuses.insert(std::ptr::from_ref(raw) as usize, status.ptr());
            game.fast_time = 1100;
        });
        let quest = collect(fake.get(), std::ptr::null_mut())[0];
        assert_eq!(quest.duration(), Some(Duration::from_secs(600)));
        assert_eq!(quest.time_left(), Some(Duration::from_secs(500)));
        fake::with(|game| game.fast_time = 5000);
        assert_eq!(quest.time_left(), Some(Duration::ZERO));
    }

    #[test]
    fn only_a_shared_task_has_members() {
        let fake = Fake::<CTaskManager>::new();
        let manager = fake.get();
        add_quest(manager, 0, 10, "Alone");
        let shared = manager.SharedTaskEntries(0);
        shared.set_TaskID(20);
        shared.set_TaskSystem(TaskSystemType::cTaskSystemTypeSharedQuest);
        let (first, second) = (Fake::<SharedTaskPlayerInfo>::new(), Fake::<SharedTaskPlayerInfo>::new());
        first.get().set_Name(set_name("Ann"));
        first.get().set_pNext(second.ptr());
        second.get().set_Name(set_name("Bob"));
        second.get().set_Role(eSharedTaskPlayerRole::eSharedTaskRoleLeader);
        let all = collect(manager, first.ptr());
        let names: Vec<_> = all[0].members().map(|m| m.name()).collect();
        assert_eq!(names, ["Ann", "Bob"]);
        assert_eq!(all[0].leader().map(|m| m.name()), Some("Bob".into()));
        assert_eq!(all[1].members().count(), 0);
    }

    #[test]
    fn objective_kinds_are_named_as_macroquest_names_them() {
        assert_eq!(ObjectiveKind::from_raw(10).name(), "UseSkill");
        assert_eq!(ObjectiveKind::from_raw(-1).name(), "Unknown");
        assert_eq!(ObjectiveKind::from_raw(99).name(), "NULL");
        assert_eq!(ObjectiveKind::from_raw(0).name(), "None");
    }
}
