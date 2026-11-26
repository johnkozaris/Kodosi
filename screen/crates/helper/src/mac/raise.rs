//! Brings a watched window to the front before a viewer clicks into it, so that the click lands
//! on it rather than on a window above it.

use std::ptr::{self, NonNull};

use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFRetained, CFString, CFType, CGPoint, CGRect, CGSize, ConcreteType,
};

use super::{item, sources};

/// A window's accessibility frame lies within this many points of where the window server has it.
const NEAR: f64 = 1.0;

/// Raises the window when another one is in front at `at`; returns whether it did.
pub(super) fn raise(window: u32, at: CGPoint) -> bool {
    if sources::front_window_at(at) == Some(window) {
        return false;
    }
    let Some(placed) = sources::placed_window(window) else {
        return false;
    };
    // SAFETY: the accessibility object of the window's app.
    let app = unsafe { AXUIElement::new_application(placed.owner) };
    let Some(windows) = attribute::<CFArray>(&app, "AXWindows") else {
        return false;
    };
    let found = (0..windows.count()).find_map(|index| {
        let each = item::<AXUIElement>(&windows, index)?;
        frame(each)
            .is_some_and(|frame| near(frame, placed.bounds))
            .then_some(each)
    });
    let Some(found) = found else {
        return false;
    };
    let front: &CFType = CFBoolean::new(true);
    // SAFETY: an action that windows take and an attribute that apps take, with its type.
    unsafe {
        found.perform_action(&CFString::from_static_str("AXRaise"));
        app.set_attribute_value(&CFString::from_static_str("AXFrontmost"), front);
    }
    true
}

fn attribute<T: ConcreteType>(element: &AXUIElement, name: &'static str) -> Option<CFRetained<T>> {
    let mut value: *const CFType = ptr::null();
    // SAFETY: `value` is valid to write, and a copied value comes retained.
    let value = unsafe {
        let error = element
            .copy_attribute_value(&CFString::from_static_str(name), NonNull::from(&mut value));
        if error != AXError::Success {
            return None;
        }
        CFRetained::from_raw(NonNull::new(value.cast_mut())?)
    };
    value.downcast::<T>().ok()
}

fn frame(window: &AXUIElement) -> Option<CGRect> {
    let (position, size) = (
        attribute::<AXValue>(window, "AXPosition")?,
        attribute::<AXValue>(window, "AXSize")?,
    );
    let (mut origin, mut extent) = (CGPoint::ZERO, CGSize::ZERO);
    // SAFETY: each pointer is valid to write a value of the type that it names.
    let read = unsafe {
        position.value(AXValueType::CGPoint, NonNull::from(&mut origin).cast())
            && size.value(AXValueType::CGSize, NonNull::from(&mut extent).cast())
    };
    read.then_some(CGRect::new(origin, extent))
}

fn near(one: CGRect, other: CGRect) -> bool {
    (one.origin.x - other.origin.x).abs() <= NEAR
        && (one.origin.y - other.origin.y).abs() <= NEAR
        && (one.size.width - other.size.width).abs() <= NEAR
        && (one.size.height - other.size.height).abs() <= NEAR
}
