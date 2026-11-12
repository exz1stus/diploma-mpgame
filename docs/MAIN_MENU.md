# Main menu handoff

The client now opens on a menu. It keeps the existing server connection flow: **Play** becomes available after the server has sent the initial world state. **Escape** opens or closes the menu during play. Options include borderless fullscreen and the existing network diagnostics panel.

The current scene is drawn in `crates/client/src/game/menu.rs` by `paint_backdrop`. It is temporary vector art, so the game runs without any texture files. The editable title is `GAME_TITLE` in the same file. The window title is set in `crates/client/src/main.rs`.

For a final background image, a 16:9 composition at 1920×1080 or larger is a useful starting point. Keep the left third quiet and dark enough for the menu panel; put the main subject in the right half. Export a PNG for an initial integration pass. The buttons, status text, and options are UI elements and should stay separate from the artwork so they remain readable at different window sizes.
