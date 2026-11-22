# Inventory integration playtest

See [CODE_MAP.md](CODE_MAP.md) for the inventory plugins and module responsibilities.

The game now loads `items.ron`, `inventories.ron`, `rules.ron`, and `ui.ron`.
The dedicated inventory scene also validates and loads `demo_scene.ron` once.
Set `INVENTORY_CONFIG_DIR` to use another directory with these files.

Build from the repository root:

```sh
cargo build -p server -p client
```

Start the inventory server in one terminal:

```sh
PROTOCOL_ID=1 TICKRATE=60 TEST_SCENE=inventory cargo run -p server
```

In each of two other terminals, start a client and join `127.0.0.1:5000`:

```sh
PROTOCOL_ID=1 cargo run -p client
```

On Linux, the existing launcher also supports:

```sh
PROTOCOL_ID=1 TICKRATE=60 ./scripts/dev.sh -n 2 -s inventory
```

Use the same `PROTOCOL_ID` on server and clients. Existing `.env` defaults can
provide the other settings. The inventory demo starts players with empty
inventories at half of the configured maximum health. Other scenes start them
at maximum health. Fixture contents are created only at startup, never on open.

- `I`: show or close your inventory.
- `E`: interact with the nearest eligible chest or ground stack.
- Drag stacks between slots, or Shift-click to transfer to/from the open chest.
- Select a carried stack and choose an amount to drop or give to a nearby player.
- Right-click a carried item for consume/drop actions.
- `G`: drop the selected amount. Drops currently use the upward direction.
- Escape cancels an active stack drag, then closes inventory, then opens the menu.

Both players can inspect the same chest. Start with 12 wood, 8 stone, 3 healing
herbs in the chest and 5 iron ore on the ground. Move, drop, collect, and give
items; after consuming one herb, totals should be unchanged except herbs = 2.
Healing applies once and is capped at the configured maximum health.

Disconnecting or dying spills carried inventory once. Rejoining starts a new
inventory session; previously pending commands and selections are cleared.
Use the network diagnostics delay controls for a delayed-message playtest.

This playtest is a manual check. The full automated inventory concurrency and
duplication suite remains separate work.
