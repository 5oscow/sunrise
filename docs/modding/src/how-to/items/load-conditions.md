# Registered data load conditions

Implement `LoadCondition` as a handler that reads the condition's JSON, then
register it from
`Hooks::register_load_conditions` using its `RegistrationContext` and
`event.conditions.register(Identifier::parse("testmod:flag").unwrap(), Flag)`.
Registration happens once per mod load, with one handler per id and a read-only
`RegistrationContext`. Any mod may register an unused ID in any namespace:
mod B can register `a:x` while mod A registers `a:y`. A built-in ID (such as
`neoforge:mod_loaded`) cannot be replaced, and a duplicate ID names both mods
in the error. A registration error refuses admission even if you ignore it.

```rust,ignore
struct Flag;
impl mod_api::LoadCondition for Flag {
    fn test(&self, args: &mod_api::serde_json::Value)
        -> Result<bool, mod_api::ConditionError>
    {
        args["value"].as_bool().ok_or_else(|| mod_api::ConditionError {
            message: "value must be a boolean".into(),
        })
    }
}
```

A recipe can use the registered name alongside the built-in conditions:

```json
{
  "type": "minecraft:crafting_shapeless",
  "ingredients": ["minecraft:flint"],
  "result": {"id": "minecraft:stick"},
  "neoforge:conditions": [{"type": "testmod:flag", "value": true}]
}
```

The handler receives a borrowed raw JSON value: the condition's object without
`type`. It reads and validates its own arguments, retaining JSON booleans,
numbers, strings, lists and objects. Invalid mod input must return
`ConditionError`; handlers must not panic while reading it. `ConditionError`
has explicit Wire encoding. Native and WebAssembly registrants cross the same generated route.
Handlers run in the server's candidate data reload, before the entries become
visible. Results are retained for each entry and condition payload in that
candidate, so repeated readers do not call the handler again. A new reload
reevaluates the entry. Handlers must not mutate live data. A native handler
panic becomes a named reload error ("handler panicked"), keeping the previous
usable data. A wasm trap follows the runtime's normal failure policy.

`true` loads the entry; `false` omits it, without exposing a lower pack's
version. A callback error refuses the reload and names the file. The previous
usable recipe and resource generation stays installed.

Registering a foreign ID does not give the registering mod ownership of its
namespace. A mod's own namespace has the existing stricter handling if it
registers a handler there: unknown names in that namespace refuse the reload,
naming the file, condition and owning mod. This applies at every depth inside
`not`, `and` and `or`, even beside an already false or true branch.

An unknown name in any other namespace also makes its file absent, wherever it
appears inside `not`, `and` or `or`. Evaluation visits every branch, so
`or(true, othermod:unknown)` leaves the file out. An ERROR-level diagnostic
names the file and distinct unknown IDs. Recipe loading also refuses the load
for those unknown IDs.

The public [NeoForge data load condition documentation](https://docs.neoforged.net/docs/resources/server/conditions/)
describes `ICondition` and the top-level `neoforge:conditions` array.
