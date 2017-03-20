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

use glium::DisplayBuild;
use glium::glutin::{WindowBuilder};
use glium::backend::glutin_backend::GlutinFacade;

use state::{Action, State};

pub struct App<Data> {
    display: GlutinFacade,
    data: Data
}

impl<Data> App<Data> {
    pub fn new<WindowInit, DataInit>(f: WindowInit, g: DataInit) -> App<Data> 
        where 
            WindowInit: FnOnce(WindowBuilder) -> WindowBuilder,
            DataInit: FnOnce(&GlutinFacade) -> Data {

        let display = f(WindowBuilder::new()).build_glium().unwrap();
        let data = g(&display);

        App {
            display: display,
            data: data
        }
    }

    fn handle_events<S: State<Data>>(&mut self, mut state: S) -> Option<S> {
        loop {
            let event = if let Some(event) = self.display.poll_events().next() {
                event
            } else {
                break
            };

            state = match state.handle_event(self, event.clone()) {
                Action::Continue => state,
                Action::Done(state) => state,
                Action::Quit => return None,
            }
        }

        Some(state)
    }

    pub fn run<S: State<Data>>(&mut self, fps: u32, mut state: S) {
        let mut accum = Duration::from_millis(0);
        let mut prev = Instant::now();

        let spf = Duration::from_millis((1000.0 / fps as f64) as u64);

        while let Some(next) = self.handle_events(state) {
            state = next;
            state.handle_render(self);

            let now = Instant::now();
            accum += now - prev;
            prev = now;

            while accum >= spf {
                accum -= spf;

                state.handle_tick(self);
            }

            sleep(spf - accum);
        }
    }

    pub fn data(&self) -> &Data {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut Data {
        &mut self.data
    }

    pub fn display(&self) -> &GlutinFacade {
        &self.display
    }
}

