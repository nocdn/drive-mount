//! Runs a callback when macOS terminates the app: Cmd+Q, the app menu's or
//! the Dock's Quit, logging out, and shutting down. AppKit exits the process
//! straight after, so the code after the event loop never runs on that path.

use std::ptr::NonNull;

use block2::RcBlock;
use objc2_app_kit::NSApplicationWillTerminateNotification;
use objc2_foundation::{NSNotification, NSNotificationCenter};

pub fn on_terminate(callback: impl Fn() + 'static) {
    let block = RcBlock::new(move |_: NonNull<NSNotification>| callback());
    // SAFETY: the notification is posted on the main thread, which runs the
    // block synchronously before AppKit exits; the observer token lives for
    // the rest of the process.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSApplicationWillTerminateNotification),
            None,
            None,
            &block,
        )
    };
    std::mem::forget(observer);
}
