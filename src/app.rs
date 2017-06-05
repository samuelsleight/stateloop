//////////////////////////////////////////////////////////////////////////////
//  File: stateloop/app.rs
//////////////////////////////////////////////////////////////////////////////
//  Copyright 2017 Samuel Sleight
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
//////////////////////////////////////////////////////////////////////////////

use std::time::{Duration, Instant};
use std::sync::Arc;
use std::thread::sleep;

use vulkano::instance::Instance;
use vulkano_win::VkSurfaceBuild;
use winit::EventsLoop;

use winit::Event as WinitEvent;

pub use vulkano::instance::InstanceCreationError;
pub use vulkano_win::{Window, CreationError};
pub use winit::WindowBuilder;

pub use winit::WindowEvent as Event;

use state::{Action, State};

pub struct App<D> {
    event_loop: EventsLoop,
    data: Data<D>
}

pub struct Data<D> {
    window: Window,
    data: D
}

#[derive(Debug)]
pub enum Error {
    WindowCreation(CreationError),
    InstanceCreation(InstanceCreationError)
}

impl<D> App<D> {
    pub fn new<WindowInit, DataInit>(instance: Arc<Instance>, f: WindowInit, g: DataInit) -> Result<App<D>, Error>
        where 
            WindowInit: FnOnce(WindowBuilder) -> WindowBuilder,
            DataInit: FnOnce(&Window) -> D {

        let event_loop = EventsLoop::new();
        let window = f(WindowBuilder::new())
            .build_vk_surface(&event_loop, instance)
            .map_err(Error::WindowCreation)?;

        let data = g(&window);

        Ok(App {
            event_loop: event_loop,
            data: Data {
                window: window,
                data: data
            }
        })
    }

    fn handle_events<S: State<D>>(&mut self, mut state: S) -> Option<S> {
        let mut quit = false;

        let event_loop = &self.event_loop;
        let data = &mut self.data;

        event_loop.poll_events(|e| {
            let WinitEvent::WindowEvent {
                window_id: _,
                event,
            } = e;

            state = match state.handle_event(data, event) {
                Action::Continue => state,
                Action::Done(state) => state,
                Action::Quit => {
                    quit = true;
                    state
                }
            }
        });

        if quit {
            None
        } else {
            Some(state)
        }
    }

    pub fn run<S: State<D>>(&mut self, fps: u32, mut state: S) {
        let mut accum = Duration::from_millis(0);
        let mut prev = Instant::now();

        let spf = Duration::from_millis((1000.0 / fps as f64) as u64);

        while let Some(next) = self.handle_events(state) {
            state = next;
            state.handle_render(&mut self.data);

            let now = Instant::now();
            accum += now - prev;
            prev = now;

            while accum >= spf {
                accum -= spf;

                state.handle_tick(&mut self.data);
            }

            sleep(spf - accum);
        }
    }

    pub fn data(&self) -> &Data<D> {
        &self.data
    }
}

impl<D> Data<D> {
    pub fn data(&self) -> &D {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut D {
        &mut self.data
    }

    pub fn window(&self) -> &Window {
        &self.window
    }
}

