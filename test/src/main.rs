#![feature(trace_macros)]

#[macro_use] extern crate stateloop;
#[macro_use] extern crate glium;

use stateloop::app::App;
use stateloop::state::Action;

use glium::Surface;
use glium::glutin::Event;

states! {
    State {
        MainHandler Main(),
        TestHandler Test(test: usize)
    }
}

impl MainHandler for App<()> {
    fn handle_event(&mut self, event: Event) -> Action<State> {
        match event {
            Event::Closed => Action::Quit,
            _ => Action::Continue
        }
    }

    fn handle_tick(&mut self) {}

    fn handle_render(&self) {
        let mut target = self.display().draw();
        target.clear_color(0.3, 0.3, 0.3, 1.0);
        target.clear_depth(1.0);
        target.finish().unwrap();
    }
}

impl TestHandler for App<()> {
    fn handle_event(&mut self, _: Event, _: usize) -> Action<State> {
        Action::Done(State::Main())
    }

    fn handle_tick(&mut self, _: usize) {}

    fn handle_render(&self, _: usize) {
        let mut target = self.display().draw();
        target.clear_color(0.3, 0.3, 0.3, 1.0);
        target.clear_depth(1.0);
        target.finish().unwrap();
    }
}

fn main() {
    App::new(
        |builder| builder
            .with_title("States Test")
            .with_dimensions(500, 500),

        |_| ()
    )
        .run(60, State::Test(15))
}
