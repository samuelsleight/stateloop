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
use std::thread::sleep;

use winit::Event as WinitEvent;

pub use winit::{EventsLoop, Window, WindowBuilder};
pub use winit::WindowEvent as Event;

use state::{Action, State};

pub struct App<D, W> {
    event_loop: EventsLoop,
    data: Data<D, W>
}

pub struct Data<D, W> {
    window: W,
    pub data: D
}

impl<D, W> App<D, W> {
    pub fn new<WindowInit, DataInit, E>(f: WindowInit, g: DataInit) -> Result<App<D, W>, E>
        where 
            WindowInit: FnOnce(&EventsLoop) -> Result<W, E>,
            DataInit: FnOnce(&W) -> D {

        let event_loop = EventsLoop::new();
        let window = f(&event_loop)?;
        let data = g(&window);

        Ok(App {
            event_loop: event_loop,
            data: Data {
                window: window,
                data: data
            }
        })
    }

    fn handle_events<S: State<D, W>>(&mut self, mut state: S) -> Option<S> {
        let mut quit = false;

        let event_loop = &mut self.event_loop;
        let data = &mut self.data;

        event_loop.poll_events(|e| {
            if let WinitEvent::WindowEvent {
                window_id: _,
                event,
            } = e {
                state = match state.handle_event(data, event) {
                    Action::Continue => state,
                    Action::Done(state) => state,
                    Action::Quit => {
                        quit = true;
                        state
                    }
                }
            }
        });

        if quit {
            None
        } else {
            Some(state)
        }
    }

    pub fn run<S: State<D, W>>(&mut self, fps: u32, mut state: S) {
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
}

impl<D, W> Data<D, W> {
    pub fn window(&self) -> &W {
        &self.window
    }
}

