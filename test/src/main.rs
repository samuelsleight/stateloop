#![feature(trace_macros)]

#[macro_use] extern crate stateloop;
#[macro_use] extern crate vulkano;
#[macro_use] extern crate vulkano_shader_derive;
extern crate vulkano_win;

use std::{ptr, mem};
use std::rc::Rc;
use std::sync::Arc;
use std::cell::{Cell, RefCell, UnsafeCell};

use stateloop::app::{App, Data, Event, Window, WindowBuilder};
use stateloop::state::Action;

use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::device::{Device, DeviceExtensions, Queue};
use vulkano::framebuffer::{Framebuffer, Subpass, FramebufferAbstract, RenderPassAbstract};
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::image::SwapchainImage;
use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};
use vulkano::pipeline::viewport::Viewport;
use vulkano::swapchain::{self, PresentMode, SurfaceTransform, Surface, Swapchain,AcquireError, SwapchainCreationError};
use vulkano::sync::{now, GpuFuture, FlushError};

use vulkano_win::VkSurfaceBuild;

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
    data: RefCell<RendererData>
}

struct RendererData {
    device: Arc<Device>,
    queue: Arc<Queue>,
    swapchain: Arc<Swapchain<Window>>,
    images: Vec<Arc<SwapchainImage<Window>>>,

    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    pipeline: Arc<GraphicsPipelineAbstract + Sync + Send>,
    render_pass: Arc<RenderPassAbstract + Send + Sync>,
    framebuffers: Option<Vec<Arc<FramebufferAbstract + Sync + Send>>>,

    dimensions: [u32; 2],
    frame_future: UnsafeCell<Box<GpuFuture>>,
    recreate_swapchain: bool
}

impl MainHandler for Data<Renderer, Arc<Surface<Window>>> {
    fn handle_event(&mut self, event: Event) -> Action<State> {
        match event {
            Event::Closed => Action::Quit,
            _ => Action::Continue
        }
    }

    fn handle_tick(&mut self) {}

    fn handle_render(&self) {
        let mut renderer = self.data.data.borrow_mut();

        let mut frame_future = unsafe { 
            let ptr = renderer.frame_future.get();
            ptr::read(ptr)
        };

        frame_future.cleanup_finished();

        loop {
            if renderer.recreate_swapchain {
                renderer.dimensions = self.window().capabilities(renderer.device.physical_device())
                    .expect("failed to get surface capabilities")
                    .current_extent
                    .unwrap();

                let (new_swapchain, new_images) = match renderer.swapchain.recreate_with_dimension(renderer.dimensions) {
                    Ok(r) => r,
                    Err(SwapchainCreationError::UnsupportedDimensions) => continue,
                    Err(err) => panic!("{:?}", err)
                };

                mem::replace(&mut renderer.swapchain, new_swapchain);
                mem::replace(&mut renderer.images, new_images);

                renderer.framebuffers = None;
                renderer.recreate_swapchain = false;
            }

            if renderer.framebuffers.is_none() {
                let new_framebuffers = Some(renderer.images.iter().map(|image| {
                    Arc::new(Framebuffer::start(renderer.render_pass.clone())
                        .add(image.clone())
                        .unwrap()
                        .build()
                        .unwrap())
                })
                    .map(|fb| fb as Arc<FramebufferAbstract + Sync + Send>)
                    .collect::<Vec<_>>());

                mem::replace(&mut renderer.framebuffers, new_framebuffers);
            }

            let (image_num, acquire_future) = match swapchain::acquire_next_image(renderer.swapchain.clone(), None) {
                Ok(r) => r,
                Err(AcquireError::OutOfDate) => {
                    renderer.recreate_swapchain = true;
                    continue
                },
                Err(err) => panic!("{:?}", err)
            };

            let command_buffer = AutoCommandBufferBuilder::primary_one_time_submit(renderer.device.clone(), renderer.queue.family())
                .unwrap()
                .begin_render_pass(
                    renderer.framebuffers.as_ref().unwrap()[image_num].clone(),
                    false,
                    vec![[1.0, 0.0, 1.0, 1.0].into()]
                )
                .unwrap()
                .draw(
                    renderer.pipeline.clone(),
                    DynamicState {
                        line_width: None,
                        viewports: Some(vec![Viewport {
                              origin: [0.0, 0.0],
                              dimensions: [renderer.dimensions[0] as f32, renderer.dimensions[1] as f32],
                              depth_range: 0.0 .. 1.0,
                        }]),
                        scissors: None
                    },
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
                .then_signal_fence_and_flush();

            let end_future = match future {
                Ok(future) => Box::new(future) as Box<_>,
                Err(FlushError::OutOfDate) => {
                    renderer.recreate_swapchain = true;
                    Box::new(now(renderer.device.clone())) as Box<_>
                },
                Err(e) => Box::new(now(renderer.device.clone())) as Box<_>
            };

            unsafe {
                let ptr = renderer.frame_future.get();
                ptr::write(ptr, end_future);
            }

            break
        }
    }
}

impl TestHandler for Data<Renderer, Arc<Surface<Window>>> {
    fn handle_event(&mut self, _: Event, _: usize) -> Action<State> {
        Action::Done(State::Main())
    }

    fn handle_tick(&mut self, _: usize) {}

    fn handle_render(&self, _: usize) {}
}

fn init_vulkan(instance: Arc<Instance>, window: &Arc<Surface<Window>>) -> Renderer {
    for device in PhysicalDevice::enumerate(&instance) {
        println!("Device: {} (type: {:?})", device.name(), device.ty());
    }

    // Select physical device
    let physical = PhysicalDevice::enumerate(&instance)
        .next()
        .expect("No device found");

    println!("Using device: {} (type: {:?})", physical.name(), physical.ty());

    // Choose gpu queue
    let queue = physical.queue_families().find(|&queue| {
        queue.supports_graphics() && window.is_supported(queue).unwrap_or(false)
    })
        .expect("No queue family found");

    // Build vulkano device object
    let (device, mut queues) = {
        let device_ext = DeviceExtensions {
            khr_swapchain: true,
            ..DeviceExtensions::none()
        };

        Device::new(
            physical, 
            physical.supported_features(), 
            &device_ext,
            [(queue, 0.5)].iter().cloned()
        )
            .expect("Failed to create device")
    };

    let queue = queues.next().unwrap();

    let mut dimensions;

    // Create swapchain
    let (swapchain, images) = {
        let caps = window.capabilities(physical)
            .expect("Failed to get surface capabilities");

        let alpha = caps.supported_composite_alpha.iter().next().unwrap();
        let format = caps.supported_formats[0].0;

        dimensions = caps.current_extent.unwrap_or([1024, 768]);

        Swapchain::new(
            device.clone(), 
            window.clone(),
            caps.min_image_count,
            format,
            dimensions,
            1,
            caps.supported_usage_flags,
            &queue,
            SurfaceTransform::Identity,
            alpha,
            PresentMode::Fifo,
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
        #[path = "shader.glsl"]
        struct Dummy;
    }

    let vs = vs::Shader::load(device.clone()).expect("failed to create shader module");
    let fs = fs::Shader::load(device.clone()).expect("failed to create shader module");

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
    let pipeline = Arc::new(GraphicsPipeline::start()
        .vertex_input_single_buffer::<Vertex>()
        .vertex_shader(vs.main_entry_point(), ())
        .triangle_list()
        .viewports_dynamic_scissors_irrelevant(1)
        .fragment_shader(fs.main_entry_point(), ())
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
        .build(device.clone())
        .unwrap());

    /*
    // Create framebuffers
    let framebuffers = images.iter().map(|image| {
        Arc::new(Framebuffer::start(render_pass.clone())
            .add(image.clone()).unwrap()
            .build().unwrap()) as Arc<FramebufferAbstract + Send + Sync>
    }).collect();
    */

    Renderer {
        data: RefCell::new(RendererData {
            device: device.clone(),
            queue,
            swapchain,
            images,

            vertex_buffer,
            pipeline: pipeline as Arc<GraphicsPipelineAbstract + Send + Sync>,
            render_pass: render_pass as Arc<RenderPassAbstract + Send + Sync>,
            framebuffers: None,

            dimensions,
            frame_future: UnsafeCell::new(Box::new(now(device.clone())) as Box<GpuFuture>),
            recreate_swapchain: false
        })
    }
}

fn main() {
    let instance = {
        let extensions = vulkano_win::required_extensions();

        Instance::new(None, &extensions, None)
            .unwrap()
    };

    let i = instance.clone();

    App::new(
        |event_loop| WindowBuilder::new()
            .with_title("States Test")
            .with_dimensions(500, 500)
            .build_vk_surface(event_loop, i),

        |window| init_vulkan(instance, window)
    )
        .unwrap()
        .run(60, State::Test(15))
}
