//! The helper on macOS: `AppKit` runs the main thread for the notice and the pointer, the link to
//! the host has a thread of its own, and `ScreenCaptureKit` hands frames over on a dispatch queue.

#![expect(
    clippy::future_not_send,
    reason = "the link's thread runs its own futures, with Objective-C objects that stay there"
)]

mod app;
mod capture;
mod clipboard;
mod hand;
mod input;
mod keys;
mod layout;
mod pointer;
mod power;
mod raise;
mod session;
mod sources;
mod trust;

use std::{
    ffi::OsString,
    ptr,
    sync::{Mutex, PoisonError},
};

use block2::{DynBlock, RcBlock};
use objc2::MainThreadMarker;
use objc2_core_foundation::{
    CFArray, CFDictionary, CFIndex, CFString, CFType, CGRect, ConcreteType,
};
use objc2_core_graphics::CGRectMakeWithDictionaryRepresentation;
use objc2_foundation::NSError;
use tokio::sync::oneshot;

pub(crate) fn run() {
    let (Some(main), Some((link, token))) = (
        MainThreadMarker::new(),
        arguments(std::env::args_os().skip(1)),
    ) else {
        return;
    };
    app::run(main, link, token);
}

/// `--link <name> --token <token>`; Launch Services may add arguments of its own.
fn arguments(mut arguments: impl Iterator<Item = OsString>) -> Option<(String, String)> {
    let (mut link, mut token) = (None, None);
    while let Some(argument) = arguments.next() {
        if argument == "--link" {
            link = arguments.next().and_then(|link| link.into_string().ok());
        } else if argument == "--token" {
            token = arguments.next().and_then(|token| token.into_string().ok());
        }
    }
    link.zip(token)
}

/// Makes a call that reports through a completion block, and waits until it does.
async fn completed(call: impl FnOnce(&DynBlock<dyn Fn(*mut NSError)>)) -> Result<(), String> {
    let (done, finished) = oneshot::channel();
    let done = Mutex::new(Some(done));
    let block = RcBlock::new(move |error: *mut NSError| {
        // SAFETY: the error is null or valid during the call.
        let problem = unsafe { error.as_ref() }.map(describe);
        let done = done.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(done) = done {
            drop(done.send(problem));
        }
    });
    call(&block);
    match finished.await {
        Ok(None) => Ok(()),
        Ok(Some(problem)) => Err(problem),
        Err(_) => Err("macOS did not answer".to_owned()),
    }
}

fn describe(error: &NSError) -> String {
    error.localizedDescription().to_string()
}

/// A dictionary's value for a key, when it has the type that the caller expects.
fn value<'a, T: ConcreteType>(dictionary: &'a CFDictionary, key: &CFString) -> Option<&'a T> {
    // SAFETY: the dictionaries that the helper reads map strings to Core Foundation objects.
    let value = unsafe {
        dictionary
            .value(ptr::from_ref(key).cast())
            .cast::<CFType>()
            .as_ref()
    }?;
    value.downcast_ref()
}

/// An array's item, when it has the type that the caller expects.
fn item<T: ConcreteType>(array: &CFArray, index: CFIndex) -> Option<&T> {
    if !(0..array.count()).contains(&index) {
        return None;
    }
    // SAFETY: the index is in the array, and the arrays that the helper reads hold Core
    // Foundation objects.
    let item = unsafe { array.value_at_index(index).cast::<CFType>().as_ref() }?;
    item.downcast_ref()
}

fn rect(dictionary: &CFDictionary) -> Option<CGRect> {
    let mut rect = CGRect::default();
    // SAFETY: `rect` is valid to write.
    let read = unsafe { CGRectMakeWithDictionaryRepresentation(Some(dictionary), &raw mut rect) };
    read.then_some(rect)
}
