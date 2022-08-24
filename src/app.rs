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

use std::thread::sleep;
use std::time::{Duration, Instant};

use winit::event_loop::ControlFlow;
use winit::platform::run_return::EventLoopExtRunReturn;

pub use winit::{
    event::WindowEvent as Event,
    event_loop::EventLoop,
    window::{Window, WindowBuilder},
};

use crate::error::{AppError, MaybeResult};
use crate::state::{Action, State};

pub struct App<D, W> {
    event_loop: EventLoop<()>,
    data: Data<D, W>,
}

pub struct Data<D, W> {
    window: W,
    pub data: D,
}

impl<D, W> App<D, W> {
    pub fn new<WindowInit, DataInit, R1, R2>(
        f: WindowInit,
        g: DataInit,
    ) -> Result<App<D, W>, AppError<R1::Error, R2::Error>>
    where
        R1: MaybeResult<W>,
        R2: MaybeResult<D>,
        WindowInit: FnOnce(&EventLoop<()>) -> R1,
        DataInit: FnOnce(&W) -> R2,
    {
        let event_loop = EventLoop::new();
        let window = f(&event_loop).as_result().map_err(AppError::WindowError)?;
        let data = g(&window).as_result().map_err(AppError::DataError)?;

        Ok(App {
            event_loop,
            data: Data { window, data },
        })
    }

    fn handle_events<S: State<D, W>>(&mut self, mut state: S) -> Option<S> {
        let mut quit = false;

        let event_loop = &mut self.event_loop;
        let data = &mut self.data;

        event_loop.run_return(|event, _, flow| {
            *flow = ControlFlow::Exit;

            if let winit::event::Event::WindowEvent {
                window_id: _,
                event,
            } = event
            {
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
