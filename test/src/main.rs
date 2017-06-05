#![feature(trace_macros)]

#[macro_use] extern crate stateloop;
extern crate vulkano;
extern crate vulkano_win;

use stateloop::app::{App, Data, Event};
use stateloop::state::Action;

use vulkano::instance::Instance;

states! {
    State {
        MainHandler Main(),
        TestHandler Test(test: usize)
    }
}

struct Renderer {

}

impl MainHandler for Data<()> {
    fn handle_event(&mut self, event: Event) -> Action<State> {
        match event {
            Event::Closed => Action::Quit,
            _ => Action::Continue
        }
    }

    fn handle_tick(&mut self) {}

    fn handle_render(&self) {
    }
}

impl TestHandler for Data<()> {
    fn handle_event(&mut self, _: Event, _: usize) -> Action<State> {
        Action::Done(State::Main())
    }

    fn handle_tick(&mut self, _: usize) {}

    fn handle_render(&self, _: usize) {
    }
}

fn main() {
    let instance = {
        let extensions = vulkano_win::required_extensions();

        Instance::new(None, &extensions, None)
            .unwrap()
    };

    App::new(
        instance,

        |builder| builder
            .with_title("States Test")
            .with_dimensions(500, 500),

        |_| ()
    )
        .unwrap()
        .run(60, State::Test(15))
}
