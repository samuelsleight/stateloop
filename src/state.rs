//////////////////////////////////////////////////////////////////////////////
//  File: stateloop/state.rs
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

use app::{Data, Event};

#[derive(Copy, Clone)]
pub enum Action<S> {
    Continue,
    Done(S),
    Quit,
}

pub trait State<D>: Copy {
    fn handle_event(self, app: &mut Data<D>, event: Event) -> Action<Self>;
    fn handle_tick(self, app: &mut Data<D>);
    fn handle_render(self, app: &Data<D>);
}

#[macro_export]
macro_rules! states {
    ($enum:ident { $($trait:ident $name:ident($($arg:ident: $t:ty),*)),+ }) => {
        states! { as_item
            #[derive(Copy, Clone)]
            pub enum $enum {
                $($name($($t,)*),)+
            }
        }

        $(pub trait $trait {
            fn handle_event(&mut self, event: Event $(, $arg: $t)*) -> $crate::state::Action<$enum>;
            fn handle_tick(&mut self $(, $arg: $t)*);
            fn handle_render(&self $(, $arg: $t)*);
        })+

        states! { as_item
            impl<D> $crate::state::State<D> for $enum where $crate::app::Data<D>: $($trait +)+ Sized {
                fn handle_event(self, app: &mut $crate::app::Data<D>, event: Event) -> $crate::state::Action<$enum> {
                    match self {
                        $($enum::$name($($arg),*) => $trait::handle_event(app, event $(, $arg)*),)+
                    }
                }

                fn handle_tick(self, app: &mut $crate::app::Data<D>) {
                    match self {
                        $($enum::$name($($arg),*) => $trait::handle_tick(app $(, $arg)*),)+
                    }
                }

                fn handle_render(self, app: &$crate::app::Data<D>) {
                    match self {
                        $($enum::$name($($arg),*) => $trait::handle_render(app $(, $arg)*),)+
                    }
                }
            }
        }
    };

    (trait_bounds $trait:ident) => { $trait };
    (trait_bounds $trait:ident $(, $traits:ident)+) => { $trait + states!(trait_bounds $($traits),+) };

    (as_item $t:item) => { $t }
}

