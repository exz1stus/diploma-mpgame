# Inventory code map

Start with the `mod.rs` files. They register the inventory plugins and point to
the smaller modules. The server owns item quantities; the client sends requests
and displays confirmed state.

## Server: game rules

[`crates/server/src/game/inventory/`](../../crates/server/src/game/inventory/)

| File | Responsibility |
| --- | --- |
| `mod.rs` | `InventoryPlugin`, service resource, and fixed-tick scheduling |
| `actions.rs` | Collect, drop, transfer, consume, and container-open requests |
| `access.rs` | Ownership, interaction distance, and quantity checks |
| `transactions.rs` | Plan changes, check capacity/revisions, and commit together |
| `locks.rs` | Reserve touched slots and ground stacks for a transaction |
| `sessions.rs` | Operation IDs, cached replies, replay protection, and rate limits |
| `network.rs` | Receive requests and send results during the server tick |
| `views.rs` | Build and publish the inventories each player may see |
| `lifecycle.rs` | Ground-item spawning and inventory cleanup before despawning |

## Client: controls and presentation

[`crates/client/src/game/inventory/`](../../crates/client/src/game/inventory/)

| File | Responsibility |
| --- | --- |
| `mod.rs` | `InventoryPlugin` and registration of its smaller plugins |
| `state.rs` | Confirmed views, selected slot, drag state, and pending request |
| `input.rs` | Keyboard controls and nearby chest/item interaction |
| `network.rs` | Submit requests and retry the same operation after a timeout |
| `ui.rs` | Inventory window, amount selector, and drop/give controls |
| `slots.rs` | Slot grids, dragging, Shift-click transfers, and context menus |
| `rendering.rs` | Chest and ground-item world markers |

## Shared: data used by both

[`crates/shared/src/inventory/`](../../crates/shared/src/inventory/)

| File | Responsibility |
| --- | --- |
| `mod.rs` | Public imports such as `shared::inventory::Inventory` |
| `components.rs` | Inventories, slots, stacks, chests, and ground items |
| `protocol.rs` | Commands, replies, errors, and confirmed inventory views |
| `definitions.rs` | Types for the editable RON files |
| `config.rs` | Load and validate settings; construct inventories from profiles |
| `fixtures.rs` | Load and validate the inventory demo scene |

## Common changes

- **Item names, stack sizes, healing:** edit [items.ron](items.ron).
- **Inventory capacity:** edit [inventories.ron](inventories.ron).
- **Interaction distance and request limits:** edit [rules.ron](rules.ron).
- **Keys, grid columns, slot sizes, colors:** edit [ui.ron](ui.ron).
- **Demo chest contents and positions:** edit [demo_scene.ron](demo_scene.ron).
- **Add an operation:** start in shared `protocol.rs`, handle it in server
  `actions.rs` through `transactions.rs`, then expose it through client input/UI.
- **Connect generated world chests:** start with
  [`test_scenes/inventory.rs`](../../crates/server/src/game/test_scenes/inventory.rs)
  as a spawning example. Keep contents associated with a stable world object so
  loading its terrain chunk again does not grant the items again.

Plugin registration is in server
[`game/mod.rs`](../../crates/server/src/game/mod.rs) and client
[`main.rs`](../../crates/client/src/main.rs).
See [PLAYTEST.md](PLAYTEST.md) for launch commands and controls.
