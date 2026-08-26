//! A drag preview that survives a drop must animate towards its new layout position, not
//! the one it had before the drop.
//!
//! `commit_box_tree` reads the dragged element's `world_transform` to detect whether its
//! layout position has moved, and if so retargets the return animation's "natural
//! position" at the new spot. But `set_world_position` (used every frame to make the
//! preview follow the cursor, and later to animate it back) replaces the node's world
//! transform outright, so as long as an override from a previous frame is still in
//! effect, `world_transform` only ever echoes back exactly what was last applied: it can
//! never reveal an independent layout change happening underneath, such as a same-key
//! `dyn_stack` row reordered to a new sibling slot while it is being dragged (same key,
//! same `ElementId`, moved rather than destroyed and recreated). The natural position
//! then stays wherever it was captured before the very first override was ever applied
//! (the row's pre-drag slot), for the entire drag and the whole return animation: visually
//! indistinguishable from a cancelled drag, even though the drop was accepted and the
//! model was updated.

use std::{cell::RefCell, rc::Rc, time::Duration};

use floem::{
    HasViewId, ViewId,
    event::{DragConfig, DragEndEvent, listener},
    headless::HeadlessHarness,
    prelude::*,
    views::{Decorators, Empty, dyn_stack},
};
use floem_test::TestRoot;

const ROW_HEIGHT: f64 = 20.0;
const ANIMATION_DURATION: Duration = Duration::from_millis(300);

#[test]
fn a_same_key_reorder_animates_the_drag_preview_towards_its_new_slot() {
    let root = TestRoot::new();
    let order: RwSignal<Vec<u32>> = RwSignal::new(vec![0, 1, 2]);
    let ids: Rc<RefCell<Vec<(u32, ViewId)>>> = Rc::new(RefCell::new(Vec::new()));

    let recorded = ids.clone();
    let view = dyn_stack(
        move || order.get(),
        |key: &u32| *key,
        move |key| {
            let row = Empty::new();
            recorded.borrow_mut().push((key, row.view_id()));
            row.style(|s| s.width_full().height(ROW_HEIGHT))
                .draggable_with_config(|| {
                    DragConfig::default().with_animation_duration(ANIMATION_DURATION)
                })
                // Mirrors the app's own drop handler: the model reorders synchronously, in
                // response to the drop, before the pointer capture is released. Moving the
                // dragged row (key 0) to the end, regardless of where it lands within row
                // 1, keeps its release point, old slot and new slot at three distinct
                // positions, so the animation's target is unambiguous to check.
                .on_event_cont(listener::DragTargetDrop, move |_, _: &DragEndEvent| {
                    if key == 1 {
                        order.set(vec![1, 2, 0]);
                    }
                })
        },
    )
    .style(|s| s.flex_col().width_full());

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 200.0);
    harness.rebuild();

    let row0 = ids.borrow().iter().find(|(key, _)| *key == 0).unwrap().1;

    // Drag row 0 (grabbed at its own vertical centre, y = 10) down and release at y = 30,
    // the centre of row 1: release_top_left.y = 30 - 10 = 20. The drop handler above then
    // sends row 0 to the end (new slot at y = 40), leaving its stale, pre-drop slot at
    // y = 0. Release point (20), old slot (0) and new slot (40) are all distinct.
    harness.pointer_down(10.0, ROW_HEIGHT / 2.0);
    harness.pointer_move(10.0, ROW_HEIGHT / 2.0 + 4.0);
    harness.pointer_move(10.0, ROW_HEIGHT * 1.5);
    harness.rebuild();
    harness.pointer_up(10.0, ROW_HEIGHT * 1.5);
    harness.rebuild();

    // Sample mid-animation: comfortably past the first frame, comfortably short of
    // `ANIMATION_DURATION`, so the preview is still interpolating rather than settled.
    // `commit_box_tree` re-evaluates the drag preview's position on every commit while
    // `active_drag` is still set, regardless of what triggered the commit, so an unrelated
    // pointer move is enough to force one without needing the real animation-frame timer
    // (which headless tests cannot pump).
    std::thread::sleep(Duration::from_millis(50));
    harness.pointer_move(90.0, 90.0);
    harness.rebuild();

    let release_y = ROW_HEIGHT; // 20.0
    let landed_y = row0.get_visual_rect().y0;
    assert!(
        landed_y > release_y,
        "row 0 kept its own ViewId across the reorder (same key), so its drag preview \
         should be animating towards its new slot (y = 40), above the release point \
         (y = {release_y}); it was instead heading towards y = {landed_y}, i.e. back \
         towards its stale pre-drop slot (y = 0)"
    );
}
