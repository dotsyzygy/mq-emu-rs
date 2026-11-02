# Working in this repo

- Never read `crates/mq-emu/src/imgui.rs` whole. Search it (Grep), or read only the lines you need (Read with offset/limit).
- To add a binding, follow `docs/adding-a-binding.md`; don't start a session by surveying the repo.
- Keep context small; every call re-sends it. Grep, then read line ranges. Don't `cat` whole files, README.md or docs/*.md, or dump `git show` diffs.
- Research in MacroQuest's source (`$MQ_ROOT`), or any broad search, goes to an Explore subagent that returns only its conclusions (file:line and signatures), not to the main context.
- Never put functionality the API should provide into a plugin (MQ2Posse, mq2-hud, mq2-bots or the test plugin). Add it to mq-emu and release it, so every plugin gets it. A plugin reading a `${...}` path or the raw game for something mq-emu could have a getter for is a missing binding: bind it.
