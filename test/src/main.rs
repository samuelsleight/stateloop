use std::{
    cell::{RefCell, UnsafeCell},
    ptr,
    sync::Arc,
};

use stateloop::{
    app::{App, Data, Event, Window, WindowBuilder},
    state::Action,
    states,
    winit::dpi::LogicalSize,
};

use bytemuck::{Pod, Zeroable};

use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer, TypedBufferAccess},
    command_buffer::{
        AutoCommandBufferBuilder, CommandBufferUsage, RenderPassBeginInfo, SubpassContents,
    },
    device::{
        physical::{PhysicalDevice, PhysicalDeviceType},
        Device, DeviceCreateInfo, DeviceExtensions, Queue, QueueCreateInfo,
    },
    image::{view::ImageView, ImageAccess, ImageUsage, SwapchainImage},
    impl_vertex,
    instance::{Instance, InstanceCreateInfo},
    pipeline::{graphics::viewport::Viewport, GraphicsPipeline},
    render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass},
    single_pass_renderpass,
    swapchain::{
        self, AcquireError, Surface, Swapchain, SwapchainCreateInfo, SwapchainCreationError,
    },
    sync::{now, FlushError, GpuFuture},
};

use vulkano_win::VkSurfaceBuild;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Zeroable, Pod)]
struct Vertex {
    position: [f32; 2],
}

impl_vertex!(Vertex, position);

stateloop::states! {
    State {
        MainHandler Main(),
        TestHandler Test(test: usize)
    }
}

struct Renderer {
    data: RefCell<RendererData>,
}

struct RendererData {
    device: Arc<Device>,
    queue: Arc<Queue>,
    swapchain: Arc<Swapchain<Window>>,
    images: Vec<Arc<SwapchainImage<Window>>>,

    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    pipeline: Arc<GraphicsPipeline>,
    render_pass: Arc<RenderPass>,
    framebuffers: Option<Vec<Arc<Framebuffer>>>,

    viewport: Viewport,
    frame_future: UnsafeCell<Box<dyn GpuFuture>>,
    recreate_swapchain: bool,
}

impl MainHandler for Data<Renderer, Arc<Surface<Window>>> {
    fn handle_event(&mut self, event: Event) -> Action<State> {
        match event {
            Event::CloseRequested => Action::Quit,
            _ => Action::Continue,
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
                let dimensions = self.window().window().inner_size();

                let (new_swapchain, new_images) =
                    match renderer.swapchain.recreate(SwapchainCreateInfo {
                        image_extent: dimensions.into(),
                        ..renderer.swapchain.create_info()
                    }) {
                        Ok(r) => r,
                        Err(SwapchainCreationError::ImageExtentNotSupported { .. }) => continue,
                        Err(err) => panic!("{:?}", err),
                    };

                renderer.swapchain = new_swapchain;
                renderer.images = new_images;
                renderer.framebuffers = None;
                renderer.recreate_swapchain = false;
            }

            if renderer.framebuffers.is_none() {
                let [w, h] = renderer.images[0].dimensions().width_height();
                renderer.viewport.dimensions = [w as f32, h as f32];

                let new_framebuffers = Some(
                    renderer
                        .images
                        .iter()
                        .map(|image| {
                            let view = ImageView::new_default(image.clone()).unwrap();
                            Framebuffer::new(
                                renderer.render_pass.clone(),
                                FramebufferCreateInfo {
                                    attachments: vec![view],
                                    ..Default::default()
                                },
                            )
                            .unwrap()
                        })
                        .collect::<Vec<_>>(),
                );

                renderer.framebuffers = new_framebuffers;
            }

            let (image_num, suboptimal, acquire_future) =
                match swapchain::acquire_next_image(renderer.swapchain.clone(), None) {
                    Ok(r) => r,
                    Err(AcquireError::OutOfDate) => {
                        renderer.recreate_swapchain = true;
                        continue;
                    }
                    Err(err) => panic!("{:?}", err),
                };

            if suboptimal {
                renderer.recreate_swapchain = true;
            }

            let mut builder = AutoCommandBufferBuilder::primary(
                renderer.device.clone(),
                renderer.queue.family(),
                CommandBufferUsage::OneTimeSubmit,
            )
            .unwrap();

            builder
                .begin_render_pass(
                    RenderPassBeginInfo {
                        clear_values: vec![Some([1.0, 0.0, 1.0, 1.0].into())],
                        ..RenderPassBeginInfo::framebuffer(
                            renderer.framebuffers.as_ref().unwrap()[image_num].clone(),
                        )
                    },
                    SubpassContents::Inline,
                )
                .unwrap()
                .set_viewport(0, [renderer.viewport.clone()])
                .bind_pipeline_graphics(renderer.pipeline.clone())
                .bind_vertex_buffers(0, renderer.vertex_buffer.clone())
                .draw(renderer.vertex_buffer.len() as u32, 1, 0, 0)
                .unwrap()
                .end_render_pass()
                .unwrap();

            let command_buffer = builder.build().unwrap();

            let future = frame_future
                .join(acquire_future)
                .then_execute(renderer.queue.clone(), command_buffer)
                .unwrap()
                .then_swapchain_present(
                    renderer.queue.clone(),
                    renderer.swapchain.clone(),
                    image_num,
                )
                .then_signal_fence_and_flush();

            let end_future = match future {
                Ok(future) => Box::new(future) as Box<_>,
                Err(FlushError::OutOfDate) => {
                    renderer.recreate_swapchain = true;
                    Box::new(now(renderer.device.clone())) as Box<_>
                }
                Err(_) => Box::new(now(renderer.device.clone())) as Box<_>,
            };

            unsafe {
                let ptr = renderer.frame_future.get();
                ptr::write(ptr, end_future);
            }

            break;
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
    // We first need to find a physical device to use
    let device_extensions = DeviceExtensions {
        khr_swapchain: true,
        ..DeviceExtensions::none()
    };

    let (physical_device, queue_family) = PhysicalDevice::enumerate(&instance)
        .filter(|&device| {
            device
                .supported_extensions()
                .is_superset_of(&device_extensions)
        })
        .filter_map(|device| {
            device
                .queue_families()
                .find(|&queue| {
                    queue.supports_graphics() && queue.supports_surface(&window).unwrap_or(false)
                })
                .map(|queue| (device, queue))
        })
        .min_by_key(|(device, _)| match device.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            PhysicalDeviceType::VirtualGpu => 2,
            PhysicalDeviceType::Cpu => 3,
            PhysicalDeviceType::Other => 4,
        })
        .expect("No suitable device found");

    println!(
        "Using device: {} (type: {:?})",
        physical_device.properties().device_name,
        physical_device.properties().device_type
    );

    // Now construct the actual device
    let (device, mut queues) = Device::new(
        physical_device,
        DeviceCreateInfo {
            enabled_extensions: device_extensions,
            queue_create_infos: vec![QueueCreateInfo::family(queue_family)],
            ..Default::default()
        },
    )
    .expect("Failed to construct device");

    let queue = queues.next().unwrap();

    // Create swapchain
    let (swapchain, images) = {
        let surface_capabilities = physical_device
            .surface_capabilities(&window, Default::default())
            .expect("Failed to get surface capabilities");

        let image_format = Some(
            physical_device
                .surface_formats(&window, Default::default())
                .unwrap()[0]
                .0,
        );

        Swapchain::new(
            device.clone(),
            window.clone(),
            SwapchainCreateInfo {
                min_image_count: surface_capabilities.min_image_count,
                image_format,
                image_extent: surface_capabilities.current_extent.unwrap_or([1024, 768]),
                image_usage: ImageUsage::color_attachment(),
                composite_alpha: surface_capabilities
                    .supported_composite_alpha
                    .iter()
                    .next()
                    .unwrap(),
                ..Default::default()
            },
        )
        .expect("Failed to create swapchain")
    };

    // Create vertex buffer
    let vertex_buffer = {
        CpuAccessibleBuffer::from_iter(
            device.clone(),
            BufferUsage::vertex_buffer(),
            false,
            [
                Vertex {
                    position: [-0.5, -0.25],
                },
                Vertex {
                    position: [0.0, 0.5],
                },
                Vertex {
                    position: [0.25, -0.3],
                },
            ]
            .iter()
            .cloned(),
        )
        .expect("Failed to create buffer")
    };

    // Create shaders
    mod vs {
        vulkano_shaders::shader! {
            ty: "vertex",
            src: "
#version 450

layout(location = 0) in vec2 position;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
}
            "
        }
    }

    mod fs {
        vulkano_shaders::shader! {
            ty: "fragment",
            path: "shader.glsl"
        }
    }

    let vs = vs::load(device.clone()).expect("Failed to crate vertex shader");
    let fs = fs::load(device.clone()).expect("Failed to crate fragment shader");

    // Create render pass
    let render_pass = single_pass_renderpass!(
        device.clone(),
        attachments: {
            colour: {
                load: Clear,
                store: Store,
                format: swapchain.image_format(),
                samples: 1,
            }
        },
        pass: {
            color: [colour],
            depth_stencil: {}
        }
    )
    .unwrap();

    // Create pipeline
    let pipeline = GraphicsPipeline::start()
        .vertex_input_single_buffer::<Vertex>()
        .vertex_shader(vs.entry_point("main").unwrap(), ())
        .triangle_list()
        .viewports_dynamic_scissors_irrelevant(1)
        .fragment_shader(fs.entry_point("main").unwrap(), ())
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
        .build(device.clone())
        .unwrap();

    let viewport = Viewport {
        origin: [0.0, 0.0],
        dimensions: [0.0, 0.0],
        depth_range: 0.0..1.0,
    };

    Renderer {
        data: RefCell::new(RendererData {
            device: device.clone(),
            queue,
            swapchain,
            images,

            vertex_buffer,
            pipeline,
            render_pass,
            framebuffers: None,

            viewport,
            frame_future: UnsafeCell::new(Box::new(now(device.clone())) as Box<dyn GpuFuture>),
            recreate_swapchain: false,
        }),
    }
}

fn main() {
    let instance = {
        let extensions = vulkano_win::required_extensions();

        Instance::new(InstanceCreateInfo {
            enabled_extensions: extensions,
            enumerate_portability: true,
            ..Default::default()
        })
        .unwrap()
    };

    let i = instance.clone();

    App::new(
        |event_loop| {
            WindowBuilder::new()
                .with_title("States Test")
                .with_inner_size(LogicalSize::new(500, 500))
                .build_vk_surface(event_loop, i)
        },
        |window| init_vulkan(instance, window),
    )
    .unwrap()
    .run(60, State::Test(15))
}
