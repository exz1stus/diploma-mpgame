# Week 7: inventory, items, and containers

This document records the original inventory design and implementation handoff,
dated 2026-11-17. The implementation now loads the editable RON files, and the
`inventory` test scene loads the demo fixture once. See [PLAYTEST.md](PLAYTEST.md)
for launch commands and controls. The sections below preserve the design plan;
they are not a report of completed tests. The active data schema is
[`shared/src/inventory/definitions.rs`](../../crates/shared/src/inventory/definitions.rs),
with the original reference in [`config_schema.rs`](config_schema.rs).

For the current plugin layout and where to make changes, start with
[CODE_MAP.md](CODE_MAP.md).

Use one server-owned `Inventory` component on players, enemies, corpses, and
chests. Clients show confirmed state and submit intentions. Every movement of
items goes through one server transaction service, including ground pickup/drop,
direct giving, consumption, death, and container destruction.

## What your client already provides

| Existing file | Reuse or change |
| --- | --- |
| [`client/src/game/commands.rs`](../../crates/client/src/game/commands.rs) | `submit`, `PredictedCommand`, and `PendingCommands` already correlate commands and replies. Implement inventory prediction as a UI pending marker; never modify confirmed quantities in `predict`. |
| [`shared/src/commands.rs`](../../crates/shared/src/commands.rs) | Typed `NetworkCommand`, payload validation, command IDs, and results already exist. Only `Shoot` is defined. Add inventory commands, specific errors, and a result carrying committed revisions. |
| [`server/src/game/commands.rs`](../../crates/server/src/game/commands.rs) | `handle_shoot` demonstrates deriving the actor from the connection's owned entities. Reuse that authorization pattern. Its deferred despawn is not an inventory transaction implementation. |
| [`shared/src/replication.rs`](../../crates/shared/src/replication.rs) and [`shared/src/lib.rs`](../../crates/shared/src/lib.rs) | Reliable registration and serialization exist. Register public chest/item metadata; deliver private inventory contents through authorized views described below. |
| [`client/src/main.rs`](../../crates/client/src/main.rs) and [`client/src/game/menu.rs`](../../crates/client/src/game/menu.rs) | egui is already installed, with `EguiPrimaryContextPass` and an established palette. Build the slot panels with it. |
| [`client/src/game/rendering.rs`](../../crates/client/src/game/rendering.rs) | Currently adds player circles. Add chest and ground-item visuals; colored placeholders can use the RON fallback colors. |
| [`client/src/networking/session.rs`](../../crates/client/src/networking/session.rs) | Session cleanup already clears command/prediction state. Also clear inventory views, selections, drags, and pending inventory operations. |
| [`worldgen/src/objects/definition.rs`](../../crates/worldgen/src/objects/definition.rs) | RON definitions and repository validation are a useful local pattern. World object kinds and collectible item kinds should be separate namespaces. |

## Findings to address before relying on multiplayer inventory

1. **No application-level command deduplication.**
   [`server/src/networking/commands.rs`](../../crates/server/src/networking/commands.rs)
   enqueues each received envelope; `CommandContext.sequence` correlates a reply
   but is not checked against a replay ledger. Reliable transport handles packet
   retransmission, but cannot make a newly submitted duplicate game operation safe.
2. **Server tick is insufficient as an inventory revision.**
   [`client/src/networking/replication.rs`](../../crates/client/src/networking/replication.rs)
   rejects a second component update at the same tick, except for `Ownership`.
   A spawn/baseline followed by a change in that tick can leave stale state. The
   collector usually coalesces changes, but that does not cover every baseline
   and command-result path. Inventory views need their own monotonic revisions.
3. **Current replication broadcasts contents if registered naively.**
   Changes are sent to all ready clients in
   [`server/src/networking/replication.rs`](../../crates/server/src/networking/replication.rs).
   Initial snapshots in [`server/src/networking/world.rs`](../../crates/server/src/networking/world.rs)
   and spawn snapshots in [`server/src/networking/entity.rs`](../../crates/server/src/networking/entity.rs)
   also serialize all registered components. Adding owner checks only to later
   updates would still expose inventories through these other paths.
4. **Lifecycle paths currently remove players directly.**
   Shooting despawns the hit player, and players use `DisconnectPolicy::Despawn`.
   Route death and disconnect through the inventory service before despawning.
   For the Week 7 prototype, spill carried items once on disconnect and create
   loot on death. This is a proposed policy; persistence/account restoration is
   separate work. Loot containers must have enough capacity for every source slot
   or spill overflow to ground in the same transaction.
5. **Input needs one owner.** Escape currently always toggles the menu. Inventory
   should first cancel a drag/close its panel, then allow the menu to open on the
   next Escape. Consume UI clicks before shooting; check both egui capture and
   explicit inventory interaction state. Apply keyboard capture to interactions
   and movement while typing quantities. An open inventory need not stop movement.

## What to borrow from the example games

**Don't Starve Together:** its game scripts separate container behavior from a
network replica and keep container widget parameters in a separate table. The
treasure-chest entry builds a 3-by-3 layout. This is a useful model for separating
your gameplay component, client view, and editable layout. Sources are original
game-script text hosted in a community mirror, rather than an official release:
[`containers.lua`](https://github.com/taichunmin/dont-starve-together-game-scripts/blob/master/containers.lua),
[`container.lua`](https://github.com/taichunmin/dont-starve-together-game-scripts/blob/master/components/container.lua),
and [`container_replica.lua`](https://github.com/taichunmin/dont-starve-together-game-scripts/blob/master/components/container_replica.lua).
The locking scheme below is our recommendation, not a claim about DST internals.

**Factorio:** item prototypes define stack limits and container prototypes define
capacity; runtime inventories hold stacks. Its insertion API reports the actual
inserted quantity. Borrow that separation and make moved quantities explicit in
results. See the official [item prototype](https://lua-api.factorio.com/latest/prototypes/ItemPrototype.html#stack_size),
[container prototype](https://lua-api.factorio.com/latest/prototypes/ContainerPrototype.html#inventory_size),
and [inventory API](https://lua-api.factorio.com/latest/classes/LuaInventory.html#insert).
Factorio uses deterministic lockstep, as explained in its
[developer blog](https://www.factorio.com/blog/post/fff-188). Your project already
uses an authoritative server and snapshots; keep that architecture.

## Editable files and their ownership

All numbers here are proposed starting values for this project.

| File | Editable values | Who owns the loaded data |
| --- | --- | --- |
| [`items.ron`](items.ron) | Stable item IDs, names, stack limits, optional icons, fallback colors, healing effect | Server owns gameplay values; client needs display definitions |
| [`inventories.ron`](inventories.ron) | Inventory profiles, slot counts, access policy | Server |
| [`rules.ron`](rules.ron) | Interaction distances, drop offset, maximum health, request limits, reply cache size, pending timeout | Server enforces gameplay/network limits; timeout is a client presentation setting |
| [`ui.ron`](ui.ron) | Grid columns, slot size/gap, colors, key bindings | Client |
| [`demo_scene.ron`](demo_scene.ron) | One shared chest, ground pickup, positions, starting stacks | Server test scene only |
| [`config_schema.rs`](config_schema.rs) | Exact proposed Rust/Serde field types and enum variants | Shared definition code after integration |

To add an item, append a definition with a unique permanent string ID. Change
`max_stack` to tune its stack size. Set an icon to `Some("path/inside/assets.png")`
when that asset exists; `None` uses a colored placeholder. Add a new effect variant
and server handler when adding behavior beyond `Heal`.

For a larger chest, change its profile's `slots` and its UI layout's `columns`.
Rows are `ceil(slots / columns)`; incomplete final rows are valid. Gameplay
capacity must never be calculated from UI layout. An item named `iron_ore` does
not automatically become loot from the world object with the same name; add an
explicit harvesting/loot mapping when that feature exists.

During integration, add `ron = "0.12"` as a direct dependency to the crate that
loads these definitions; worldgen's dependency does not make it importable in
shared/client/server. Enable Serde's `derive` feature explicitly for the schema's
own crate. Move the schema into `shared::inventory::definitions` and load from an
explicit configurable asset directory with `std::fs::read_to_string` and
`ron::from_str`. Using `include_str!` would require rebuilding after edits.

Validate at startup: supported schema version, unknown fields, nonempty unique
IDs, nonzero capacities/stack sizes/effects, finite valid ranges, positive rate
limits/timeouts, valid UI keys, one layout per profile, and every fixture's item,
profile, quantity, slot index, and unique ID. Reject overfull fixtures and duplicate
slot assignments. The reference schema alone only checks deserialization types.
Load gameplay definitions once per server session. Coordinate a definition
version/content digest at join and reject mismatches before accepting commands.
Changing capacities or stack limits in an existing saved world will need a
migration; do not silently truncate inventory contents.

## Runtime types to agree with your colleague

These are runtime contracts to implement, separate from static item definitions:

```rust
// Illustrative types, not currently declared in the game.
struct ItemStack { item_id: ItemId, quantity: u32 }
struct InventorySlot { revision: u64, stack: Option<ItemStack> }
// Derive Component for Inventory; reuse on all inventory-bearing entities.
struct Inventory { revision: u64, profile_id: ProfileId, slots: Vec<InventorySlot> }
struct SlotRef { inventory: NetworkEntityId, slot: u16, expected_revision: u64 }
struct GroundItem { revision: u64, stack: ItemStack }
struct Chest { profile_id: ProfileId }
struct InventoryOperationId { session_epoch: u64, counter: u64 }
```

Use existing `NetworkEntityId`, never local Bevy `Entity` values on the wire.
Use permanent content keys for `ItemId`/`ProfileId` first; compact IDs can follow
once the mapping is versioned. No durability, quality, spoilage, or nested bags
are required for Week 7. If added later, their metadata must participate in stack
compatibility; matching item type alone would no longer be enough.

Increment each touched slot revision once per committed operation, including when
a slot becomes empty. Never reset a slot revision when replacing its item. Bump
each touched inventory's revision once. Ground stack quantity changes also bump
its revision. Use checked arithmetic and fail before mutation on overflow.

## Command contract

Append command IDs after existing `Shoot` (ID 1). Do not reorder the current
implicit ID list; preferably make all protocol IDs explicit as this list grows.
Every mutating command carries an `InventoryOperationId`.

| Command | Proposed payload and behavior |
| --- | --- |
| `CollectItem` | Ground entity ID, expected ground revision, requested quantity. Server derives the player's destination and plans insertion. |
| `DropItem` | Source `SlotRef`, quantity, finite aim direction. Server derives actor position and validates a nearby drop location; clients cannot choose arbitrary coordinates. |
| `TransferContainerItem` | Source `SlotRef`, destination inventory ID, optional explicit destination slot plus its expected revision, quantity. Covers chest transfers, local rearrangement, and giving. |
| `ConsumeItem` | Source `SlotRef`; consumes one item and applies its defined effect atomically. Required by the acceptance goal even though absent from the initial three-command list. |
| `OpenContainer` | Container entity ID. Checks access/range and adds this connection as a viewer; returns a full view. Multiple viewers are allowed. Opening creates no loot. |
| `CloseContainer` | Container entity ID. Removes the viewer and clears that client view. |
| `RequestInventoryResync` | Authorized inventory IDs and/or a previous operation ID. Returns current views/cached status without repeating any mutation. |

For explicit slot-to-slot moves, require both slot revisions. For automatic
insertion, the server chooses compatible partial stacks and then empty slots
from current state, so no client-selected destination revision is needed. Reject
identical source/destination slots and automatic insertion into the same inventory;
allow deliberate rearrangement with distinct explicit slots.

Start with **all-or-nothing requested quantities**: if the complete requested
amount cannot fit, reject without changing anything. A successful response still
returns `moved_quantity`. Add partial transfers later only with an explicit result
contract. Giving may remove items only from the sender's own inventory and insert
into a nearby recipient's inventory automatically. It never grants read access to
the recipient's inventory or permits withdrawing their items. Check the recipient
is alive, in range, and accepting gifts. Enemy inventories are server-only until
their loot becomes available as a corpse or ground items.

Add specific domain errors, e.g. `NotFound`, `OutOfRange`, `AccessDenied`,
`InvalidQuantity`, `StaleSlot`, `InventoryFull`, `SlotBusy`, `CannotConsume`, and
`OperationTooOld`, alongside the existing envelope/serialization errors. A healing
item at maximum health returns `CannotConsume` and is not removed. Clamp healing
to the server's maximum using checked or widened arithmetic.

## Transactions, concurrency, and slot locking

Have one server inventory queue process **all** inventory mutation types in a
defined order before `ReplicationSet::Collect`. Separate typed message readers
must feed that queue without reversing the reliable inventory request order.
An exclusive world system is a straightforward first implementation. Do not run
collect/drop/consume handlers independently against stale copies of inventory.

Each operation follows this sequence:

1. Bind the actor and session to the connection; apply an inventory request rate
   limit. Check the operation ledger before mutation. A completed duplicate gets
   its original result; the same ID with different contents is rejected.
2. Validate payload bounds, access, actor state, source revision/quantity,
   recipient/viewer permissions, and current server positions.
3. Build a complete transaction plan against current state: all source and
   destination slots, ground stack changes, health changes, and spawn/despawn work.
4. Reserve every touched slot/ground stack in a server-only lock table, in stable
   `(network_entity_id, slot_index)` order. Acquire the whole set or release it all
   and reject. Revalidate revisions under those reservations.
5. Commit the whole plan synchronously, bump revisions, and record its result in
   the operation ledger before publishing replies/updates. Release all reservations
   on success and every failure path. No asynchronous work occurs inside commit.
6. Send the reliable result and confirmed state updates to authorized viewers.

**For Week 7, locks last for one transaction, not the time a chest UI is open.**
Two players can view a chest and operate on different slots. If both request the
same source revision, the first operation commits and the second gets `StaleSlot`
with refreshed state. Per-slot checks avoid rejecting an unrelated slot merely
because another player changed the inventory's overall revision. A serial queue
already prevents interleaving; the lock table documents/enforces the touched set
and supports future parallel callers. No mutex across a network round trip is
needed. Busy/pending visuals are local pending state or responses, not a promise
that another player's mouse drag reserves an item.

If shared visible drag reservations become a separate product requirement, add
server-issued lease tokens, server-time expiry, renewal, and disconnect cleanup.
Those persistent drag leases are additional work beyond transaction-scoped locking.
The UI drag in this proposal holds only a reference; the item remains in its
authoritative source until a successful transfer.

For pickup, synchronously decrement or mark the ground stack exhausted before a
deferred despawn. `commands.entity(...).despawn()` alone is insufficient: another
request in the same loop can still find that entity. For drop, commit the debit
and creation of exactly one authoritative ground stack together. Prepare identity
allocation/spawn data before making irreversible model changes; publish only after
the complete model commit. Adapt the existing deferred `network_spawn` helper or
add an immediate server-world equivalent for this service.

The replay ledger is scoped to a server-issued connection/session epoch and an
inventory-only monotonic counter. Keep one inventory mutation in flight per client
initially. Retain a high-water mark even after `cached_results_per_session` replies
are evicted: older IDs must be rejected/resynced, never executed again. A retry
uses the same operation ID and payload, even if its transport message sequence is
new. The existing transport sequence resets during initial-state handling, so it
is not a sufficient durable operation identity. Reset the inventory counter only
on a new server-issued session epoch, not a view resync. Old epochs cannot replay.

## Replication and UI reconciliation

Keep `Inventory` as server-owned data and maintain a separate client
`InventoryViews` resource. Do not register private contents in generic broadcast
replication. Send reliable ordered view messages to the owner and authorized
container viewers, covering initial view, commit, resync, and viewer removal.
Close subscriptions when players leave range, die, disconnect, or the container
despawns. Revalidate access for each operation, even while a subscription exists.

Use a reliable `InventoryCommit` message with an operation ID and complete views
of the touched inventories visible to that recipient. Include the associated
ground-stack changes/removals, with revisions/tombstones. Apply all its patches
together to avoid showing half a transfer. Redact other players' private contents;
the sender of a gift receives the moved quantity and their own new inventory.
Public chest/ground entity metadata can use existing reliable replication, but
avoid competing writers for stack quantities. Revision-check mutable ground state
just like inventory views, including spawn/resync paths.

Full view snapshots are sufficient for these small inventories initially. Use
slot deltas later if profiling justifies them. Drop messages with an older or equal
inventory revision; newer revisions apply even within the same server tick.
An initial revision-zero view must apply when no previous view exists. Snapshot
messages and command results must share one revision-aware apply path, so a late
reply cannot overwrite a newer view. Returning only revision numbers requires
waiting for the matching view update before clearing a slot's pending state.

In `PredictedCommand::predict`, mark touched UI slots pending and retain the
operation ID. On success apply confirmed state; on rejection show the domain
error and refresh. A timeout means outcome unknown: request status/resync, keep
the operation identity, and reconcile. Never infer rollback or blindly send a
fresh command ID. Clear pending state on disconnect and obtain a fresh baseline
on reconnect. Do not include inventory actions in movement rollback/replay.

## Client panels and interactions

Render the player's grid and the open container's grid side by side with names,
counts, tooltips, selection, and pending/busy states. Empty slots remain visible.
Use `MenuState::in_game()` and `ClientState.initiated` as existing UI gates.

- `I`: toggle inventory. `E`: interact with the nearest eligible chest or pickup.
- Left click: select; dragging proposes a transfer on release. Never create a
  second real stack under the cursor.
- Shift click: quick transfer to/from the open container, using server insertion.
- Right click: context menu with consume, choose amount, drop one, and drop stack.
- `G`: drop from the selected owned slot, with an amount chosen by the UI.
- Give: choose a nearby recipient and quantity, then send the same transfer
  command with automatic insertion into that recipient's inventory.
- Escape: cancel drag, then close panels, then allow the existing game menu.

Build UI in `EguiPrimaryContextPass`. Queue intents in a resource and drain them
through the command submission path; define scheduling explicitly, since that UI
pass and the current Update networking chain are different schedules. Add fallback
chest/ground visuals without routing them through the player-specific renderer.

## Current code files and original integration order

The implementation now uses this plugin layout. See [CODE_MAP.md](CODE_MAP.md)
for responsibilities and common changes:

```text
crates/shared/src/inventory/
  mod.rs          # Public imports and IDs
  components.rs   # Inventory, InventorySlot, ItemStack, Chest, GroundItem
  protocol.rs     # Wire commands, views, outcomes, and errors
  definitions.rs  # RON schema
  config.rs       # Validated configuration loading and inventory construction
  fixtures.rs     # Validated demo scene loading
crates/server/src/game/inventory/
  mod.rs          # InventoryPlugin, service, fixed-tick schedule
  actions.rs      # Collect, drop, transfer, consume, and container access
  transactions.rs # One planner/commit path
  locks.rs        # Server-only slot/ground reservations
  sessions.rs     # Session-scoped replay ledger and rate limits
  access.rs       # Ownership, range, recipient rules
  network.rs      # Request queue and replies
  views.rs        # Authorized inventory views
  lifecycle.rs    # Ground spawning and death/disconnect cleanup
crates/client/src/game/inventory/
  mod.rs          # InventoryPlugin and module plugin registration
  state.rs        # Confirmed views, revisions, pending/drag state
  input.rs        # Keyboard controls and nearby interactions
  network.rs      # Command submission and retries
  ui.rs           # Inventory window and action controls
  slots.rs        # Player/container grids and slot interactions
  rendering.rs    # Chest and ground-item visuals
```

The original integration sequence was:

1. Agree on the shared runtime/wire contract and RON schema with your colleague.
   Move the examples to the chosen runtime assets directory and implement validated
   loading. The editable files contain definitions, never player save state.
2. Implement the server transaction service, operation ledger, access checks,
   reservations, and domain tests before wiring UI interactions.
3. Add reliable view/commit/resync messages in `shared/src/server_messages.rs`,
   route them in the client message handler, and establish the session epoch in
   the initialization handshake. Wire command registration into the existing
   shared registry and server game plugin.
4. Add the inventory component to `new_player_bundle` and enemy bundles. Add the
   one-time fixture through `server/src/game/test_scenes`, then integrate death,
   disconnect, container destruction, and subscription cleanup.
5. Add client inventory modules/resources, command prediction implementations,
   egui panels, world visuals, input routing, and all session reset paths.
6. Run the tests below and the existing relevant shared/server/client checks.
   Demonstrate the complete two-client flow under artificial latency.

Do not generate chest contents when a viewer opens it or a terrain chunk loads
again. Generated chests need a stable object-instance-to-container-state mapping,
so rematerializing a chunk reuses its contents. The included fixture avoids that
integration until the Week 7 inventory is working. No real enemy gameplay or
inventory UI currently exists; use a test enemy/corpse first.

## Acceptance tests for the duplication requirement

The required invariant per item kind is:

```text
sum(all authoritative inventory quantities) + sum(all ground quantities)
    = initial quantity + explicit spawn/grants - consumed/destroyed quantity
```

Assert this after every transaction, including failures, in randomized operation
sequences. Also assert valid slot bounds, stack limits, unique entity identities,
nonnegative counts, monotonic revisions, and no leftover reservations. Include
server-only enemy inventories in the sum. UI replicas are not additional items.

| Scenario | Required result |
| --- | --- |
| Two players take the same last chest item/revision | Exactly one succeeds; the other refreshes |
| Two players take different slots in the same chest | Both can succeed |
| Both collect the same ground stack in one tick | Only the remaining authoritative quantity can be awarded |
| Requested quantity cannot fit | Neither source nor destination changes |
| Duplicate drop/consume/give with the same operation ID | One mutation/effect; repeat receives original result |
| Replay after cached reply eviction or after reconnect | Rejected/resynced; never re-executed |
| Consume and transfer/drop the same source revision | One succeeds; effect/removal occurs at most once |
| Forged source owner, far chest, far recipient, zero/oversized quantity | Rejected without changes |
| Exact destination changed, same-slot move, incompatible stack | Rejected without changes |
| Another player changes an unrelated slot | No false source-slot conflict |
| Missing/despawned container or ground entity | Rejected and stale client view cleared |
| Spawn/view and multiple commits share a server tick | Client eventually displays the highest object revisions |
| Duplicate/late snapshots or late command replies | Newer confirmed state is preserved |
| Delay/lost reply, then status request/retry | No repeated effect; pending state resolves |
| Disconnect/death/chest destruction during a pending action | One valid lifecycle outcome; no duplicate spill or orphaned locks |
| Close/reopen chest or reload its terrain chunk | Contents are preserved; fixture/loot is not granted again |

For the supplied fixture, start both player inventories empty. Totals are wood 12,
stone 8, healing herb 3, and iron ore 5. Have both players loot, drop/pick up, give
to each other, then consume one herb while injured. Expected final totals are
unchanged except healing herb 2, and exactly one capped heal is applied. Use the
existing `NetworkSimulation`/diagnostics delay controls for the two-client demo;
use a test harness or packet proxy to exercise loss, duplicates, and reordering.

These tests make the zero-duplication acceptance condition concrete. The design
and editable files alone do not establish it. Crash/restart guarantees additionally
require durable transaction/save handling; the current menu explicitly says world
saving is not implemented.
