-- rustpeer: the Lua side of MQ2RustPluginTest's Lua checks.
--
--   /lua run rustpeer, then /rusttest selftest. /lua stop rustpeer ends it.
--
-- Run it on your other characters too, and the selftest also checks Rust and
-- Lua talking across characters.
--
-- Rust sends requests as tables with an `id`; each is answered with
-- message:reply(status, value), status 0 for known requests and 1 otherwise.

local mq = require('mq')
local actors = require('actors')

-- MQ2RustPluginTest's mailbox for Lua messages (MacroQuest drops "MQ2").
local RUST_MAILBOX = 'RustPluginTest:lua'

local me = mq.TLO.Me.CleanName()
local mailbox

-- What the last 'call_rust' got back from Rust, for 'last_reply'.
local last_reply

local function rust_on(character)
    return { character = character, mailbox = RUST_MAILBOX, absolute_mailbox = true }
end

local handlers = {}

-- The value straight back: Rust checks it survived Lua's decode and encode.
function handlers.echo(content)
    return content.value
end

-- Lua's own reading of a table, proving the fields arrived as Lua types.
function handlers.inspect(content)
    local value = content.value
    local sum = 0
    for _, n in ipairs(value.list) do
        sum = sum + n
    end
    return {
        name_type = type(value.name),
        list_len = #value.list,
        list_sum = sum,
        flag_type = type(value.flag),
        nested = value.nested and value.nested.deeper,
    }
end

-- Sends a request to Rust (on content.character, default this one) and keeps
-- the reply.
function handlers.call_rust(content)
    last_reply = nil
    mailbox:send(rust_on(content.character or me), { id = 'hello', from = 'lua', token = content.token }, function(status, reply)
        last_reply = { status = status, content = reply.content }
    end)
    return { id = 'sent' }
end

function handlers.last_reply()
    return last_reply or { id = 'none' }
end

mailbox = actors.register(function(message)
    local content = message.content
    local id = type(content) == 'table' and content.id or nil
    local handler = handlers[id]
    if handler then
        message:reply(0, handler(content, message))
    else
        message:reply(1, { error = 'unknown request', id = tostring(id) })
    end
end)

printf('\at[rustpeer]\ax listening on %s; now /rusttest selftest', me)
while true do
    mq.delay(100)
end
