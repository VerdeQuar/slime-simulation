use bevy::{
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        graph::CameraDriverLabel,
        render_asset::{RenderAssetUsages, RenderAssets},
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{
            binding_types::{storage_buffer, texture_storage_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        texture::GpuImage,
        Render, RenderApp, RenderSet,
    },
};

use bytemuck::{Pod, Zeroable};
use crossbeam_channel::{Receiver, Sender};
use iyes_perf_ui::{entries::PerfUiCompleteBundle, PerfUiPlugin};
use rand::Rng;
use std::mem::size_of;

const SHADER_ASSET_PATH: &str = "slime.wgsl";
const NUM_AGENTS: usize = 100000;
const WORKGROUP_SIZE: u32 = 32;
const WIDTH: i32 = 2560;
const HEIGHT: i32 = 1440;
const SCALE_FACTOR: i32 = 1;

#[derive(Resource, Default, Clone, Copy, ShaderType)]
struct Params {
    speed: f32,
    turn_speed: f32,
    sensor_size: i32,
    sensor_offset_distance: f32,
    sensor_angle_offset: f32,
    fade_speed: f32,
}

#[derive(Resource, Clone, ExtractResource)]
struct SlimeTexture(Handle<Image>);

#[derive(Resource, Clone)]
struct ExtractedTime {
    delta_seconds: f32,
}

impl ExtractResource for ExtractedTime {
    type Source = Time;

    fn extract_resource(source: &Self::Source) -> Self {
        Self {
            delta_seconds: source.delta_seconds(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, ShaderType, Zeroable, Pod)]
struct Agent {
    position: Vec2,
    angle: f32,
    species: u32,
}

struct GpuBuffers {
    params: UniformBuffer<Params>,
    agents: BufferVec<Agent>,
    delta_seconds: UniformBuffer<f32>,
}

struct StagingBuffers {
    agents: Buffer,
}

#[derive(Resource)]
struct ComputeBuffers {
    gpu_buffers: GpuBuffers,
    staging_buffers: StagingBuffers,
}

#[derive(Resource, Deref)]
struct MainWorldReceivers {
    agents: Receiver<Vec<Agent>>,
}

#[derive(Resource, Deref)]
struct RenderWorldSenders {
    agents: Sender<Vec<Agent>>,
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::BLACK))
        .add_plugins((DefaultPlugins, ComputePlugin))
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin)
        .add_plugins(bevy::diagnostic::EntityCountDiagnosticsPlugin)
        .add_plugins(bevy::diagnostic::SystemInformationDiagnosticsPlugin)
        .add_plugins(PerfUiPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, receive)
        .run();
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let texture = {
        let mut image = Image::new_fill(
            Extent3d {
                width: WIDTH as u32,
                height: HEIGHT as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 255],
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.usage = TextureUsages::COPY_DST
            | TextureUsages::STORAGE_BINDING
            | TextureUsages::TEXTURE_BINDING;
        images.add(image)
    };

    commands.spawn(PerfUiCompleteBundle::default());
    commands.spawn(Camera2dBundle::default());
    commands.spawn(SpriteBundle {
        sprite: Sprite {
            custom_size: Some(Vec2::new(WIDTH as f32, HEIGHT as f32)),
            ..default()
        },
        texture: texture.clone(),
        transform: Transform::from_scale(Vec3::splat(SCALE_FACTOR as f32)),
        ..default()
    });
    commands.insert_resource(SlimeTexture(texture));
}

fn receive(receiver: Res<MainWorldReceivers>) {
    if let Ok(data) = receiver.agents.try_recv() {
        // println!("{:?}", data[0].position);
    }
}

struct ComputePlugin;
impl Plugin for ComputePlugin {
    fn build(&self, _app: &mut App) {}
    fn finish(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<SlimeTexture>::default());
        app.add_plugins(ExtractResourcePlugin::<ExtractedTime>::default());

        let (sender_agents, receiver_agents) = crossbeam_channel::unbounded();
        app.insert_resource(MainWorldReceivers {
            agents: receiver_agents,
        });

        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .insert_resource(RenderWorldSenders {
                agents: sender_agents,
            })
            .init_resource::<ComputePipeline>()
            .init_resource::<ComputeBuffers>()
            .add_systems(
                Render,
                (
                    prepare_resources.in_set(RenderSet::PrepareResources),
                    prepare_bind_group
                        .in_set(RenderSet::PrepareBindGroups)
                        .run_if(not(resource_exists::<ComputeBindGroup>)),
                    map_and_read_buffer.after(RenderSet::Render),
                ),
            );

        let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(ComputeNodeLabel, ComputeNode::default());
        graph.add_node_edge(ComputeNodeLabel, CameraDriverLabel);
    }
}

impl FromWorld for ComputeBuffers {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let render_queue = world.resource::<RenderQueue>();
        let mut rng = rand::thread_rng();

        let mut gpu_buffer_params = UniformBuffer::<Params>::default();
        gpu_buffer_params.set(Params {
            speed: 50.,
            turn_speed: 0.75,
            sensor_size: 6,
            sensor_offset_distance: 15.,
            sensor_angle_offset: std::f32::consts::FRAC_PI_2,
            fade_speed: 0.003,
        });
        gpu_buffer_params.write_buffer(render_device, render_queue);

        let mut gpu_buffer_agents = BufferVec::new(BufferUsages::STORAGE | BufferUsages::COPY_SRC);
        for _ in 0..NUM_AGENTS {
            let middle = Vec2::new(WIDTH as f32, HEIGHT as f32) / 2.;
            let mut position =
                (Vec2::from_angle(rng.gen_range(-std::f32::consts::PI..std::f32::consts::PI))
                    * rng.gen_range(0. ..500.))
                    + middle;

            // position = Vec2::new(
            //     rng.gen_range(0. ..WIDTH as f32),
            //     rng.gen_range(0. ..HEIGHT as f32),
            // );

            gpu_buffer_agents.push(Agent {
                position,
                // angle: rng.gen_range(-std::f32::consts::PI..std::f32::consts::PI),
                angle: (middle - position).normalize().to_angle() + std::f32::consts::FRAC_PI_2,
                species: rng.gen_range(0..=2),
            });
        }
        gpu_buffer_agents.write_buffer(render_device, render_queue);
        let staging_buffer_agents = render_device.create_buffer(&BufferDescriptor {
            label: Some("readback_buffer"),
            size: (NUM_AGENTS * size_of::<Agent>()) as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut gpu_buffer_delta_seconds = UniformBuffer::<f32>::default();
        gpu_buffer_delta_seconds.set(1.);
        gpu_buffer_delta_seconds.write_buffer(render_device, render_queue);

        ComputeBuffers {
            gpu_buffers: GpuBuffers {
                params: gpu_buffer_params,
                agents: gpu_buffer_agents,
                delta_seconds: gpu_buffer_delta_seconds,
            },
            staging_buffers: StagingBuffers {
                agents: staging_buffer_agents,
            },
        }
    }
}

fn prepare_resources(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    extracted_time: Res<ExtractedTime>,
    mut slime_compute_buffers: ResMut<ComputeBuffers>,
) {
    slime_compute_buffers
        .gpu_buffers
        .delta_seconds
        .set(extracted_time.delta_seconds);
    slime_compute_buffers
        .gpu_buffers
        .delta_seconds
        .write_buffer(&render_device, &render_queue);
}

#[derive(Resource)]
struct ComputeBindGroup(BindGroup);

fn prepare_bind_group(
    mut commands: Commands,
    pipeline: Res<ComputePipeline>,
    render_device: Res<RenderDevice>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    slime_texture: Res<SlimeTexture>,
    buffers: Res<ComputeBuffers>,
) {
    let slime_texture_view = gpu_images.get(&slime_texture.0).unwrap();
    let bind_group = render_device.create_bind_group(
        None,
        &pipeline.bind_group_layout,
        &BindGroupEntries::sequential((
            buffers
                .gpu_buffers
                .params
                .binding()
                .expect("Buffer should have already been uploaded to the gpu"),
            &slime_texture_view.texture_view,
            buffers
                .gpu_buffers
                .agents
                .binding()
                .expect("Buffer should have already been uploaded to the gpu"),
            buffers
                .gpu_buffers
                .delta_seconds
                .binding()
                .expect("Buffer should have already been uploaded to the gpu"),
        )),
    );
    commands.insert_resource(ComputeBindGroup(bind_group));
}

#[derive(Resource)]
struct ComputePipeline {
    bind_group_layout: BindGroupLayout,
    init_pipeline: CachedComputePipelineId,
    update_agents_pipeline: CachedComputePipelineId,
    update_texture_pipeline: CachedComputePipelineId,
}

impl FromWorld for ComputePipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let bind_group_layout = render_device.create_bind_group_layout(
            None,
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    uniform_buffer::<Params>(false),
                    texture_storage_2d(
                        TextureFormat::Rgba8Unorm,
                        bevy::render::render_resource::StorageTextureAccess::ReadWrite,
                    ),
                    storage_buffer::<Vec<Agent>>(false),
                    uniform_buffer::<f32>(false),
                ),
            ),
        );
        let shader = world.load_asset(SHADER_ASSET_PATH);
        let pipeline_cache = world.resource::<PipelineCache>();

        let init_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: None,
            layout: vec![bind_group_layout.clone()],
            push_constant_ranges: Vec::new(),
            shader: shader.clone(),
            shader_defs: Vec::new(),
            entry_point: std::borrow::Cow::from("init"),
        });
        let update_agents_pipeline =
            pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
                label: None,
                layout: vec![bind_group_layout.clone()],
                push_constant_ranges: Vec::new(),
                shader: shader.clone(),
                shader_defs: Vec::new(),
                entry_point: std::borrow::Cow::from("update_agents"),
            });
        let update_texture_pipeline =
            pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
                label: None,
                layout: vec![bind_group_layout.clone()],
                push_constant_ranges: Vec::new(),
                shader: shader.clone(),
                shader_defs: Vec::new(),
                entry_point: std::borrow::Cow::from("update_texture"),
            });

        Self {
            bind_group_layout,
            init_pipeline,
            update_agents_pipeline,
            update_texture_pipeline,
        }
    }
}

fn map_and_read_buffer(
    render_device: Res<RenderDevice>,
    buffers: Res<ComputeBuffers>,
    senders: Res<RenderWorldSenders>,
) {
    let buffer_slice = buffers.staging_buffers.agents.slice(..);
    let (s, r) = crossbeam_channel::unbounded::<()>();

    buffer_slice.map_async(MapMode::Read, move |r| match r {
        Ok(_) => s.send(()).expect("Failed to send map update"),
        Err(err) => panic!("Failed to map buffer {err}"),
    });

    render_device.poll(Maintain::wait()).panic_on_timeout();

    r.recv().expect("Failed to receive the map_async message");

    {
        let buffer_view = buffer_slice.get_mapped_range();

        let data = bytemuck::cast_slice(&buffer_view).to_vec();
        senders
            .agents
            .send(data)
            .expect("Failed to send data to main world");
    }

    buffers.staging_buffers.agents.unmap();
}

#[derive(Default)]
enum ComputeState {
    #[default]
    Loading,
    Init,
    UpdateAgents,
    UpdateTexture,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct ComputeNodeLabel;

#[derive(Default)]
struct ComputeNode {
    state: ComputeState,
}

impl render_graph::Node for ComputeNode {
    fn update(&mut self, world: &mut World) {
        let pipeline = world.resource::<ComputePipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        match self.state {
            ComputeState::Loading => {
                match pipeline_cache.get_compute_pipeline_state(pipeline.init_pipeline) {
                    CachedPipelineState::Ok(_) => {
                        self.state = ComputeState::Init;
                    }
                    CachedPipelineState::Err(err) => {
                        panic!("Initializing assets/{SHADER_ASSET_PATH}:\n{err}")
                    }
                    _ => {}
                }
            }
            ComputeState::Init => {
                if let (CachedPipelineState::Ok(_), CachedPipelineState::Ok(_)) = (
                    pipeline_cache.get_compute_pipeline_state(pipeline.update_agents_pipeline),
                    pipeline_cache.get_compute_pipeline_state(pipeline.update_texture_pipeline),
                ) {
                    self.state = ComputeState::UpdateAgents;
                }
            }
            ComputeState::UpdateAgents => {
                self.state = ComputeState::UpdateTexture;
            }
            ComputeState::UpdateTexture => {
                self.state = ComputeState::UpdateAgents;
            }
        }
    }

    fn run(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), render_graph::NodeRunError> {
        let bind_group = &world.resource::<ComputeBindGroup>().0;
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<ComputePipeline>();

        {
            let mut pass = render_context
                .command_encoder()
                .begin_compute_pass(&ComputePassDescriptor::default());

            // select the pipeline based on the current state
            match self.state {
                ComputeState::Loading => {}
                ComputeState::Init => {
                    let init_pipeline = pipeline_cache
                        .get_compute_pipeline(pipeline.init_pipeline)
                        .unwrap();
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_pipeline(init_pipeline);
                    pass.dispatch_workgroups(NUM_AGENTS as u32 / WORKGROUP_SIZE, 1, 1);
                }
                ComputeState::UpdateAgents => {
                    let update_pipeline = pipeline_cache
                        .get_compute_pipeline(pipeline.update_agents_pipeline)
                        .unwrap();
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_pipeline(update_pipeline);
                    pass.dispatch_workgroups(NUM_AGENTS as u32 / WORKGROUP_SIZE, 1, 1);
                }
                ComputeState::UpdateTexture => {
                    let update_pipeline = pipeline_cache
                        .get_compute_pipeline(pipeline.update_texture_pipeline)
                        .unwrap();
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_pipeline(update_pipeline);
                    pass.dispatch_workgroups(
                        WIDTH as u32 / WORKGROUP_SIZE,
                        HEIGHT as u32 / WORKGROUP_SIZE,
                        1,
                    );
                }
            }
        }

        let slime_compute_buffers = world.resource::<ComputeBuffers>();
        render_context.command_encoder().copy_buffer_to_buffer(
            slime_compute_buffers
                .gpu_buffers
                .agents
                .buffer()
                .expect("Buffer should have already been uploaded to the gpu"),
            0,
            &slime_compute_buffers.staging_buffers.agents,
            0,
            (NUM_AGENTS * std::mem::size_of::<Agent>()) as u64,
        );
        Ok(())
    }
}
