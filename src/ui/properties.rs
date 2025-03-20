use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use obs_wrapper::prelude::*;
use obs_wrapper::source::*;

pub struct ReloadButtonState {
    reload_requested: Arc<AtomicBool>
}

impl ReloadButtonState {
    pub fn new() -> Self {
        Self {
            reload_requested: Arc::new(AtomicBool::new(false))
        }
    }
    
    pub fn get_button_property(&self) -> ObsProperty {
        let reload_requested = self.reload_requested.clone();
        
        // Create a button property that sets the flag when clicked
        ObsProperties::new()
            .add_button("Reload Shader", "Reload Shader", move || {
                reload_requested.store(true, Ordering::SeqCst);
                false // Return false to keep the properties dialog open
            })
            .to_property()
    }
    
    pub fn check_and_reset(&self) -> bool {
        // Check if reload was requested and reset the flag
        self.reload_requested.swap(false, Ordering::SeqCst)
    }
    
    pub fn get_reload_requested_handle(&self) -> Arc<AtomicBool> {
        self.reload_requested.clone()
    }
} 