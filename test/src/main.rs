#![feature(trace_macros)]

#[macro_use] extern crate stateloop;
#[macro_use] extern crate vulkano;
#[macro_use] extern crate vulkano_shader_derive;
extern crate vulkano_win;

use std::sync::Arc;
use std::cell::UnsafeCell;
use std::ptr;
use std::time::Duration;

use stateloop::app::{App, Data, Event, Window};
use stateloop::state::Action;

use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::device::{Device, Queue, DeviceExtensions};
use vulkano::swapchain::{acquire_next_image, Swapchain, SurfaceTransform};
use vulkano::image::swapchain::SwapchainImage;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::cpu_access::CpuAccessibleBuffer;
use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineParams, GraphicsPipelineAbstract};
use vulkano::pipeline::vertex::SingleBufferDefinition;
use vulkano::pipeline::input_assembly::InputAssembly;
use vulkano::pipeline::viewport::{Scissor, Viewport, ViewportsState};
use vulkano::pipeline::multisample::Multisample;
use vulkano::pipeline::depth_stencil::DepthStencil;
use vulkano::pipeline::blend::Blend;
use vulkano::framebuffer::{Subpass, Framebuffer, FramebufferAbstract};
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferBuilder, DynamicState};
use vulkano::sync::{now, GpuFuture};

#[derive(Debug, Clone)]
struct Vertex {
    position: [f32; 2]
}

impl_vertex!(Vertex, position);
        
states! {
    State {
        MainHandler Main(),
        TestHandler Test(test: usize)
    }
}

struct Renderer {
    device: Arc<Device>,
    queue: Arc<Queue>,
    swapchain: Arc<Swapchain>,

    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    pipeline: Arc<GraphicsPipelineAbstract + Sync + Send>,
    framebuffers: Vec<Arc<FramebufferAbstract + Sync + Send>>,

    frame_future: UnsafeCell<Box<GpuFuture>>,
}

impl MainHandler for Data<Renderer> {
    fn handle_event(&mut self, event: Event) -> Action<State> {
        match event {
            Event::Closed => Action::Quit,
            _ => Action::Continue
        }
    }

    fn handle_tick(&mut self) {}

    fn handle_render(&self) {
        let renderer = self.data();

        let mut frame_future = unsafe { 
            let ptr = renderer.frame_future.get();
            ptr::read(ptr)
        };

        frame_future.cleanup_finished();
        let (image_num, acquire_future) = acquire_next_image(
            renderer.swapchain.clone(),
            Duration::new(1, 0)
        ).unwrap();

        let command_buffer = AutoCommandBufferBuilder::new(renderer.device.clone(), renderer.queue.family())
            .unwrap()
            .begin_render_pass(
                renderer.framebuffers[image_num].clone(),
                false,
                vec![[1.0, 0.0, 1.0, 1.0].into()]
            )
            .unwrap()
            .draw(
                renderer.pipeline.clone(),
                DynamicState::none(),
                vec![renderer.vertex_buffer.clone()], 
                (), ()
            )
            .unwrap()
            .end_render_pass()
            .unwrap()
            .build()
            .unwrap();

        let future = frame_future
            .join(acquire_future)
            .then_execute(
                renderer.queue.clone(), 
                command_buffer
            )
            .unwrap()
            .then_swapchain_present(
                renderer.queue.clone(), 
                renderer.swapchain.clone(), 
                image_num
            )
            .then_signal_fence_and_flush()
            .unwrap();

        unsafe {
            let ptr = renderer.frame_future.get();
            ptr::write(ptr, Box::new(future) as Box<_>);
        }
    }
}

impl TestHandler for Data<Renderer> {
    fn handle_event(&mut self, _: Event, _: usize) -> Action<State> {
        Action::Done(State::Main())
    }

    fn handle_tick(&mut self, _: usize) {}

    fn handle_render(&self, _: usize) {
    }
}

fn init_vulkan(instance: Arc<Instance>, window: &Window) -> Renderer {
    // Select physical device
    let physical = PhysicalDevice::enumerate(&instance)
        .next()
        .expect("No device found");

    println!("Using device: {} (type: {:?})", physical.name(), physical.ty());

    // Choose gpu queue
    let queue = physical.queue_families().find(|&queue| {
        queue.supports_graphics() && window.surface().is_supported(queue).unwrap_or(false)
    })
        .expect("No queue family found");

    // Build vulkano device object
    let (device, mut queues) = {
        let device_ext = DeviceExtensions {
            khr_swapchain: true,
            ..DeviceExtensions::none()
        };

        Device::new(
            &physical, 
            physical.supported_features(), 
            &device_ext,
            [(queue, 0.5)].iter().cloned()
        )
            .expect("Failed to create device")
    };

    let queue = queues.next().unwrap();

    // Create swapchain
    let (swapchain, images) = {
        let caps = window.surface().capabilities(physical)
            .expect("Failed to get surface capabilities");

        let dimensions = caps.current_extent.unwrap_or([500, 500]);
        let present = caps.present_modes.iter().next().unwrap();
        let alpha = caps.supported_composite_alpha.iter().next().unwrap();
        let format = caps.supported_formats[0].0;

        Swapchain::new(
            device.clone(), 
            window.surface().clone(),
            caps.min_image_count,
            format,
            dimensions,
            1,
            caps.supported_usage_flags,
            &queue,
            SurfaceTransform::Identity,
            alpha,
            present,
            true,
            None
        )
            .expect("Failed to create swapchain")
    };

    // Create vertex buffer
    let vertex_buffer = {
        CpuAccessibleBuffer::from_iter(
            device.clone(),
            BufferUsage::vertex_buffer(),
            Some(queue.family()),
            [
                Vertex { position: [-0.5, -0.25] },
                Vertex { position: [0.0, 0.5] },
                Vertex { position: [0.25, -0.3] }
            ].iter().cloned()
        )
            .expect("Failed to create buffer")
    };

    // Create shaders
    mod vs {
        #[derive(VulkanoShader)]
        #[ty = "vertex"]
        #[src = "
#version 450

layout(location = 0) in vec2 position;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
}
"]
        struct Dummy;
    }

    mod fs {
        #[derive(VulkanoShader)]
        #[ty = "fragment"]
        #[src = "
#version 450

layout(location = 0) out vec4 f_color;

void main() {
    f_color = vec4(1.0, 0.0, 0.0, 1.0);
}
"]
        struct Dummy;
    }

    let vs = vs::Shader::load(&device).expect("failed to create shader module");
    let fs = fs::Shader::load(&device).expect("failed to create shader module");

    // Create render pass
    let render_pass = Arc::new(single_pass_renderpass!(
        device.clone(),
        attachments: {
            color: {
                load: Clear,
                store: Store,
                format: swapchain.format(),
                samples: 1,
            }
        },
        pass: {
            color: [color],
            depth_stencil: {}
        }
    ).unwrap());

    // Create pipeline
    let pipeline = Arc::new(GraphicsPipeline::new(
        device.clone(),
        GraphicsPipelineParams {
            vertex_input: SingleBufferDefinition::<Vertex>::new(),
            vertex_shader: vs.main_entry_point(),
            input_assembly: InputAssembly::triangle_list(),
            tessellation: None,
            geometry_shader: None,
            viewport: ViewportsState::Fixed {
                data: vec![(
                    Viewport {
                        origin: [0.0, 0.0],
                        depth_range: 0.0 .. 1.0,
                        dimensions: [images[0].dimensions()[0] as f32,
                                     images[0].dimensions()[1] as f32],
                    },
                    Scissor::irrelevant()
                )],
            },
            raster: Default::default(),
            multisample: Multisample::disabled(),
            fragment_shader: fs.main_entry_point(),
            depth_stencil: DepthStencil::disabled(),
            blend: Blend::pass_through(),
            render_pass: Subpass::from(render_pass.clone(), 0).unwrap(),
        }
    ).unwrap());

    // Create framebuffers
    let framebuffers = images.iter().map(|image| {
        Arc::new(Framebuffer::start(render_pass.clone())
            .add(image.clone()).unwrap()
            .build().unwrap()) as Arc<FramebufferAbstract + Send + Sync>
    }).collect();

    Renderer {
        device: device.clone(),
        queue: queue,
        swapchain: swapchain,

        vertex_buffer: vertex_buffer,
        pipeline: pipeline as Arc<GraphicsPipelineAbstract + Send + Sync>,
        framebuffers: framebuffers,

        frame_future: UnsafeCell::new(Box::new(now(device.clone())) as Box<GpuFuture>)
    }
}

fn main() {
    let instance = {
        let extensions = vulkano_win::required_extensions();

        Instance::new(None, &extensions, None)
            .unwrap()
    };

    App::new(
        instance.clone(),

        |builder| builder
            .with_title("States Test")
            .with_dimensions(500, 500),

        |window| init_vulkan(instance, window)
    )
        .unwrap()
        .run(60, State::Test(15))
}
