//! The Apple Pencil, read from UIKit: winit's UIKit view forwards a Pencil touch like a finger's
//! and says nothing of a Pencil hovering above the screen (iPad Pro M2 and later, iPadOS 16.1+).
//! Two recognisers are added to winit's view:
//!
//! - [`TouchObserver`] sees every touch first and tells the Pencil's from a finger's
//!   (`UITouch.type`). It never recognises a gesture, so the touches still reach winit.
//! - a `UIHoverGestureRecognizer` reports where the Pencil hovers.
//!
//! Both only record into [`crate::pencil_input`] and wake the UI; [`crate::app::IosApp`] reads that
//! each frame.
use std::cell::RefCell;

use eframe::egui;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{NSObject, NSSet};
use objc2_ui_kit::{UIEvent, UIGestureRecognizer, UIGestureRecognizerState, UIHoverGestureRecognizer, UITouch, UITouchType, UIView};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::pencil_input;

thread_local! {
    // UIKit keeps a recogniser's target and the recogniser itself only weakly from the view's side
    // of things we use; these keep them for the life of the app. Main thread only.
    static KEPT: RefCell<Vec<Retained<AnyObject>>> = const { RefCell::new(Vec::new()) };
    static WAKE: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
}

fn wake() {
    WAKE.with(|slot| {
        if let Ok(slot) = slot.try_borrow()
            && let Some(ctx) = slot.as_ref()
        {
            ctx.request_repaint();
        }
    });
}

/// Whether any of `touches` is the Pencil's.
fn has_pencil(touches: &NSSet<UITouch>) -> bool {
    touches.allObjects().iter().any(|t| t.r#type() == UITouchType::Pencil)
}

define_class!(
    // SAFETY: UIGestureRecognizer's subclass contract is to override the touch methods and, to claim
    // a gesture, set its state; this class does neither beyond observing, so UIKit's recognition
    // is unchanged. It has no ivars and is created and used on the main thread.
    #[unsafe(super = UIGestureRecognizer)]
    #[thread_kind = MainThreadOnly]
    #[name = "FigCraftPencilTouchObserver"]
    struct TouchObserver;

    impl TouchObserver {
        #[unsafe(method(touchesBegan:withEvent:))]
        fn began(&self, touches: &NSSet<UITouch>, _event: &UIEvent) {
            if has_pencil(touches) {
                pencil_input::with(|p| p.touch(true));
                wake();
            }
        }

        #[unsafe(method(touchesEnded:withEvent:))]
        fn ended(&self, touches: &NSSet<UITouch>, _event: &UIEvent) {
            if has_pencil(touches) {
                pencil_input::with(|p| p.touch(false));
                wake();
            }
        }

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn cancelled(&self, touches: &NSSet<UITouch>, _event: &UIEvent) {
            if has_pencil(touches) {
                pencil_input::with(|p| p.touch(false));
                wake();
            }
        }
    }
);

define_class!(
    // SAFETY: NSObject has no subclass invariants; the class has no ivars and `hover:` is called by
    // UIKit on the main thread with the recogniser it was added to.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "FigCraftPencilHoverTarget"]
    struct HoverTarget;

    impl HoverTarget {
        #[unsafe(method(hover:))]
        fn hover(&self, recogniser: &UIHoverGestureRecognizer) {
            let state = recogniser.state();
            let at = if state == UIGestureRecognizerState::Began || state == UIGestureRecognizerState::Changed {
                recogniser.view().map(|view| {
                    let p = recogniser.locationInView(Some(&view));
                    egui::pos2(p.x as f32, p.y as f32)
                })
            } else {
                None
            };
            pencil_input::with(|p| p.hover(at));
            wake();
        }
    }
);

/// Add the Pencil's recognisers to winit's view. `Err` says why not; the app works without them (the
/// Pencil then acts as a finger).
pub fn install(cc: &eframe::CreationContext<'_>) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("the Pencil recognisers are installed on the main thread")?;
    let native = cc.winit_window().ok_or("eframe did not supply its native window")?;
    let handle = native.window_handle().map_err(|error| error.to_string())?;
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
        return Err("eframe supplied a non-UIKit window".into());
    };
    // SAFETY: the borrowed handle is valid while `native` and its handle guard live; winit owns this
    // UIView and it is used on the main thread.
    let view: &UIView = unsafe { handle.ui_view.cast::<UIView>().as_ref() };
    WAKE.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            *slot = Some(cc.egui_ctx.clone());
        }
    });

    let observer: Retained<TouchObserver> = {
        let this = mtm.alloc::<TouchObserver>().set_ivars(());
        // SAFETY: `init` is UIGestureRecognizer's designated no-target initialiser.
        unsafe { msg_send![super(this), init] }
    };
    // The observer must never delay or cancel the touches winit receives.
    observer.setCancelsTouchesInView(false);
    observer.setDelaysTouchesBegan(false);
    observer.setDelaysTouchesEnded(false);
    view.addGestureRecognizer(&observer);

    let target: Retained<HoverTarget> = {
        let this = mtm.alloc::<HoverTarget>().set_ivars(());
        // SAFETY: NSObject's `init`.
        unsafe { msg_send![super(this), init] }
    };
    // SAFETY: `target` implements `hover:` taking the recogniser, as the action selector says; it is
    // kept alive below because a recogniser does not retain its target.
    let hover = unsafe {
        UIHoverGestureRecognizer::initWithTarget_action(mtm.alloc::<UIHoverGestureRecognizer>(), Some(&*target as &AnyObject), Some(sel!(hover:)))
    };
    view.addGestureRecognizer(&hover);
    KEPT.with(|kept| {
        if let Ok(mut kept) = kept.try_borrow_mut() {
            kept.push(Retained::into_super(Retained::into_super(target)));
            kept.push(Retained::into_super(Retained::into_super(observer)).into());
        }
    });
    log::info!("Apple Pencil touch and hover recognisers installed");
    Ok(())
}
