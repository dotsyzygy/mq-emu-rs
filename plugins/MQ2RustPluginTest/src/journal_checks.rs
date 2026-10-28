//! Selftest checks for your tasks, friends list, tribute and achievements:
//! mq-emu's reading of each against MacroQuest's `${Task}`, `${Friends}`,
//! `${Me.TributeActive}` (and the favor members) and `${Achievement}`.
//!
//! What you don't have skips: tasks without a task, friend names with an
//! empty list, and achievements on a server that sends none (the client then
//! never has them ready, and the achievement window won't open).

use mq_emu::{AchievementState, GameState, Mq, ObjectiveKind, Plugin, QuestKind};

use crate::checks::{Check, Report, same_text};
use crate::game_checks::{all_match, id_or_null};

/// The most achievements and categories compared one by one; a full set is
/// thousands, each a few `${...}` reads.
const MAX_ACHIEVEMENTS_COMPARED: usize = 200;
/// A name no one has on their friends list.
const NOT_A_FRIEND: &str = "Rusttestnotafriend";

/// A boolean as MacroQuest shows it.
fn tf(value: bool) -> &'static str {
    if value { "TRUE" } else { "FALSE" }
}

pub fn run<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    if mq.game_state() != GameState::InGame {
        report.print(mq, "tasks, friends, tribute and achievements", Check::Skip("needs to be in game".into()));
        return;
    }
    tasks(mq, report);
    friends(mq, report);
    tribute(mq, report);
    achievements(mq, report);
    report.print(
        mq,
        "no advanced loot window on this client",
        Check::that(mq.window("AdvancedLootWnd").is_none(), || "the client has AdvancedLootWnd: advanced loot can be bound".into()),
    );
}

fn tasks<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let quests: Vec<_> = mq.quests().collect();
    let after = quests.len() + 1;
    let past_the_end = mq.parse(&format!("${{Task[{after}].ID}}"));
    report.print(
        mq,
        "quests() ends where ${Task[n]} does",
        Check::that(past_the_end == "NULL" || past_the_end == "0", || format!("${{Task[{after}].ID}} is {past_the_end:?}")),
    );
    if quests.is_empty() {
        report.print(mq, "tasks", Check::Skip("no tasks: get one to compare them".into()));
        return;
    }
    let rows = quests.iter().enumerate().flat_map(|(i, quest)| {
        let n = i + 1;
        let kind = match quest.kind() {
            QuestKind::Solo => "Quest",
            QuestKind::Shared => "Shared",
        };
        let row = mq.quest_window_row(quest).map(|row| row + 1);
        [
            (format!("task {n} title"), quest.title(), format!("${{Task[{n}].Title}}")),
            (format!("task {n} id"), quest.id().to_string(), format!("${{Task[{n}].ID}}")),
            (format!("task {n} type"), kind.to_owned(), format!("${{Task[{n}].Type}}")),
            (format!("task {n} timer"), quest.time_left().map_or(0, |left| left.as_secs()).to_string(), format!("${{Task[{n}].Timer.TotalSeconds}}")),
            (format!("task {n} step"), id_or_null(quest.step().map(|o| o.index() + 1)), format!("${{Task[{n}].Step.Index}}")),
            (format!("task {n} window row"), id_or_null(row), format!("${{Task[{n}].WindowIndex}}")),
            (format!("task {n} by title"), quest.id().to_string(), format!("${{Task[={}].ID}}", quest.title())),
        ]
    });
    report.print(mq, "quests() match ${Task[n]} (title, id, type, timer, step, window row)", all_match(mq, rows));

    let rows = quests.iter().enumerate().flat_map(|(i, quest)| {
        let n = i + 1;
        quest.objectives().flat_map(move |objective| {
            let m = objective.index() + 1;
            let path = |member: &str| format!("${{Task[{n}].Objective[{m}].{member}}}");
            let status = match objective.current_count() {
                _ if objective.is_done() => "Done".to_owned(),
                Some(count) => format!("{count}/{}", objective.required_count()),
                None => "NULL".to_owned(),
            };
            let zone = match objective.zone_id() {
                Some(id) => (id.0.to_string(), path("Zone.ID")),
                None => ("ALL".to_owned(), path("Zone")),
            };
            let kind = match objective.kind() {
                ObjectiveKind::Other(_) => "NULL",
                kind => kind.name(),
            };
            [
                (format!("task {n} objective {m} text"), objective.text(), path("Instruction")),
                (format!("task {n} objective {m} type"), kind.to_owned(), path("Type")),
                (format!("task {n} objective {m} status"), status, path("Status")),
                (format!("task {n} objective {m} count"), id_or_null(objective.current_count()), path("CurrentCount")),
                (format!("task {n} objective {m} required"), objective.required_count().to_string(), path("RequiredCount")),
                (format!("task {n} objective {m} optional"), tf(objective.is_optional()).to_owned(), path("Optional")),
                (format!("task {n} objective {m} zone"), zone.0, zone.1),
                (format!("task {n} objective {m} item"), objective.required_item(), path("RequiredItem")),
                (format!("task {n} objective {m} skill"), objective.required_skill(), path("RequiredSkill")),
                (format!("task {n} objective {m} spell"), objective.required_spell(), path("RequiredSpell")),
                (format!("task {n} objective {m} switch"), objective.dz_switch_id().to_string(), path("DZSwitchID")),
            ]
        })
    });
    report.print(mq, "task objectives match ${Task[n].Objective[m]}", all_match(mq, rows));

    // MacroQuest gives the shared task's members for every task; mq-emu only
    // for the shared one.
    match quests.iter().position(|quest| quest.kind() == QuestKind::Shared) {
        Some(i) => {
            let n = i + 1;
            let quest = quests[i];
            let members: Vec<_> = quest.members().collect();
            let mut rows = vec![
                ("members".to_owned(), members.len().to_string(), format!("${{Task[{n}].Members}}")),
                ("leader".to_owned(), id_or_null(quest.leader().map(|m| m.name())), format!("${{Task[{n}].Leader}}")),
            ];
            rows.extend(members.iter().enumerate().map(|(j, member)| (format!("member {}", j + 1), member.name(), format!("${{Task[{n}].Member[{}].Name}}", j + 1))));
            report.print(mq, "shared task members match ${Task[n].Member[m]}", all_match(mq, rows));
        }
        None => report.print(mq, "shared task members", Check::Skip("no shared task".into())),
    }
}

fn friends<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let Some(friends) = mq.friends() else {
        report.print(mq, "friends() = ${Friends}", same_text("NULL", mq.parse("${Friends}")));
        return;
    };
    report.print(mq, "friends().len() = ${Friends}", same_text(friends.len(), mq.parse("${Friends}")));
    if friends.is_empty() {
        report.print(mq, "friend names", Check::Skip("your friends list is empty: add someone to compare names".into()));
    } else {
        let rows = friends.iter().enumerate().flat_map(|(i, name)| {
            let n = i + 1;
            [
                (format!("friend {n}"), name.clone(), format!("${{Friends.Friend[{n}]}}")),
                (format!("friend {n} by name"), tf(mq.is_friend(&name.to_uppercase())).to_owned(), format!("${{Friends.Friend[{name}]}}")),
            ]
        });
        report.print(mq, "friends() match ${Friends.Friend[n]}, and by name ignoring case", all_match(mq, rows));
    }
    let theirs = mq.parse(&format!("${{Friends.Friend[{NOT_A_FRIEND}]}}"));
    report.print(
        mq,
        "is_friend() is false for someone not on the list",
        Check::that(!mq.is_friend(NOT_A_FRIEND) && theirs == "NULL", || format!("mq-emu {}, MacroQuest {theirs:?}", mq.is_friend(NOT_A_FRIEND))),
    );
}

fn tribute<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let Some(tribute) = mq.tribute() else {
        report.print(mq, "tribute()", Check::Fail("None in game".into()));
        return;
    };
    let rows = [
        ("active".to_owned(), tf(tribute.active).to_owned(), "${Me.TributeActive}".to_owned()),
        ("current favor".to_owned(), tribute.current_favor.to_string(), "${Me.CurrentFavor}".to_owned()),
        ("career favor".to_owned(), tribute.career_favor.to_string(), "${Me.CareerFavor}".to_owned()),
        ("active cost".to_owned(), id_or_null(tribute.active_cost), "${Me.ActiveFavorCost}".to_owned()),
        ("timer".to_owned(), (tribute.time_left.as_millis() / 6000).to_string(), "${Me.TributeTimer.Ticks}".to_owned()),
    ];
    report.print(mq, "tribute() matches ${Me} (active, favor, cost, timer)", all_match(mq, rows));
}

fn achievements<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let Some(all) = mq.achievements() else {
        report.print(mq, "achievements()", Check::Fail("None in game".into()));
        return;
    };
    report.print(mq, "achievements().is_ready() = ${Achievement.Ready}", same_text(tf(all.is_ready()), mq.parse("${Achievement.Ready}")));
    if !all.is_ready() {
        report.print(mq, "achievements", Check::Skip("the server sent no achievements".into()));
        return;
    }
    let rows = [
        ("count".to_owned(), all.len().to_string(), "${Achievement.AchievementCount}".to_owned()),
        ("categories".to_owned(), all.category_count().to_string(), "${Achievement.CategoryCount}".to_owned()),
        ("points".to_owned(), all.points().to_string(), "${Achievement.Points}".to_owned()),
        ("completed".to_owned(), all.completed_count().to_string(), "${Achievement.CompletedAchievements}".to_owned()),
        ("total".to_owned(), all.total_count().to_string(), "${Achievement.TotalAchievements}".to_owned()),
    ];
    report.print(mq, "achievements() totals match ${Achievement}", all_match(mq, rows));

    let me = mq.me().map(|me| me.name()).unwrap_or_default();
    let rows = all.iter().take(MAX_ACHIEVEMENTS_COMPARED).flat_map(|achievement| {
        let n = achievement.index() + 1;
        let path = |member: &str| format!("${{Achievement.AchievementByIndex[{n}].{member}}}");
        let state = match achievement.state() {
            AchievementState::Completed => "COMPLETED",
            AchievementState::Open => "OPEN",
            AchievementState::Locked => "LOCKED",
            AchievementState::Hidden => "HIDDEN",
        };
        let mut rows = vec![
            (format!("achievement {n} id"), achievement.id().to_string(), path("ID")),
            (format!("achievement {n} name"), achievement.name(), path("Name")),
            (format!("achievement {n} points"), achievement.points().to_string(), path("Points")),
            (format!("achievement {n} state"), state.to_owned(), path("State")),
            (format!("achievement {n} objectives"), achievement.objectives().count().to_string(), path("ObjectiveCount")),
            (format!("achievement {n} by id"), n.to_string(), format!("${{Achievement[{}].Index}}", achievement.id())),
            (format!("achievement {n} link"), achievement.link(&me), path("Link")),
        ];
        for objective in achievement.objectives() {
            let id = objective.id();
            let path = |member: &str| path(&format!("Objective[{id}].{member}"));
            rows.extend([
                (format!("achievement {n} objective {id} text"), objective.description(), path("Description")),
                (format!("achievement {n} objective {id} count"), objective.count().to_string(), path("Count")),
                (format!("achievement {n} objective {id} required"), objective.required_count().to_string(), path("RequiredCount")),
                (format!("achievement {n} objective {id} done"), tf(objective.is_completed()).to_owned(), path("Completed")),
            ]);
        }
        rows
    });
    report.print(mq, "achievements match ${Achievement.AchievementByIndex[n]}", all_match(mq, rows));

    let rows = all.categories().take(MAX_ACHIEVEMENTS_COMPARED).flat_map(|category| {
        let n = category.index() + 1;
        let path = |member: &str| format!("${{Achievement.CategoryByIndex[{n}].{member}}}");
        [
            (format!("category {n} id"), category.id().to_string(), path("ID")),
            (format!("category {n} name"), category.name(), path("Name")),
            (format!("category {n} achievements"), category.achievements().count().to_string(), path("AchievementCount")),
            (format!("category {n} children"), category.children().count().to_string(), path("CategoryCount")),
            (format!("category {n} points"), category.points().to_string(), path("Points")),
            (format!("category {n} total"), category.total_count().to_string(), path("TotalAchievements")),
        ]
    });
    report.print(mq, "categories match ${Achievement.CategoryByIndex[n]}", all_match(mq, rows));
}
