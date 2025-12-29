# Components Crate

## egui State Persistence

When using `CollapsingState` (or similar egui state objects), you **must call `store()`** to persist state across frames:

```rust
let mut state = CollapsingState::load_with_default_open(ctx, id, default_open);

if response.clicked() {
    state.toggle(ui);
}

// Critical! Without this, state is lost on next frame
state.store(ctx);
```

The `toggle()` method only modifies the local struct - it does not persist to the context. egui's built-in `CollapsingHeader` handles this internally in its `show_body_*` methods, but when using `CollapsingState` directly, you must call `store()` yourself.
