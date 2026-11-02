//! The menu bar item: a template icon with Settings and Quit.

use std::cell::RefCell;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, Sel};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem};
use objc2_foundation::{MainThreadMarker, NSData, NSObject, NSSize, NSString, NSTimer};

use super::{OnAction, TrayAction};

const ICON_PNG: &[u8] = include_bytes!("../../icons/menu-bar-template.png");
const ICON_POINTS: f64 = 18.0;

struct TargetIvars {
    on_action: OnAction,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TargetIvars]
    struct Target;

    unsafe impl NSObjectProtocol for Target {}

    impl Target {
        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: &AnyObject) {
            (self.ivars().on_action)(TrayAction::OpenSettings);
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: &AnyObject) {
            (self.ivars().on_action)(TrayAction::Quit);
        }
    }
);

impl Target {
    fn new(on_action: OnAction, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars { on_action });
        unsafe { msg_send![super(this), init] }
    }
}

struct Controller {
    _item: Retained<NSStatusItem>,
    _menu: Retained<NSMenu>,
    _target: Retained<Target>,
    quit: Retained<NSMenuItem>,
}

thread_local! {
    static STATUS_ITEM: RefCell<Option<Controller>> = const { RefCell::new(None) };
}

/// Schedules the item's creation on the main run loop, which runs once
/// Rune's event loop (and with it AppKit) has started.
pub(super) fn install_when_running(on_action: OnAction) {
    let block = RcBlock::new(move |_: NonNull<NSTimer>| {
        if let Err(err) = install(on_action.clone()) {
            eprintln!("Could not create the menu bar item: {err}");
        }
    });
    // SAFETY: the timer runs its block on this (main) thread's run loop.
    let _ = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.0, false, &block) };
}

fn install(on_action: OnAction) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("the menu bar item needs the main thread")?;
    let target = Target::new(on_action, mtm);
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(crate::ui::APP_NAME));
    add_item(&menu, &target, "Settings", sel!(openSettings:), mtm);
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    let quit = add_item(&menu, &target, "Quit", sel!(quit:), mtm);

    let item = NSStatusBar::systemStatusBar().statusItemWithLength(-2.0);
    let button = item.button(mtm).ok_or("the status item has no button")?;
    let data = NSData::with_bytes(ICON_PNG);
    let image =
        NSImage::initWithData(NSImage::alloc(), &data).ok_or("could not decode the icon")?;
    image.setSize(NSSize::new(ICON_POINTS, ICON_POINTS));
    image.setTemplate(true);
    image.setAccessibilityDescription(Some(&NSString::from_str(crate::ui::APP_NAME)));
    button.setImage(Some(&image));
    button.setToolTip(Some(&NSString::from_str(crate::ui::APP_NAME)));
    item.setMenu(Some(&menu));

    STATUS_ITEM.with(|slot| {
        *slot.borrow_mut() = Some(Controller {
            _item: item,
            _menu: menu,
            _target: target,
            quit,
        });
    });
    Ok(())
}

fn add_item(
    menu: &NSMenu,
    target: &Target,
    title: &str,
    action: Sel,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::new(),
        )
    };
    unsafe { item.setTarget(Some(target)) };
    menu.addItem(&item);
    item
}

pub(super) fn set_quitting() {
    STATUS_ITEM.with(|slot| {
        if let Some(controller) = slot.borrow().as_ref() {
            controller.quit.setTitle(&NSString::from_str("Quitting..."));
            controller.quit.setEnabled(false);
        }
    });
}
