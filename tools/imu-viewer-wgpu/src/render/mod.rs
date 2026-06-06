use bytemuck::{Pod, Zeroable};
use smartimu::{ImuId, Quaternion};
use wgpu::{
    BindGroup, Buffer, Color, CommandEncoder, Device, FragmentState, MultisampleState,
    PipelineCompilationOptions, PrimitiveState, PrimitiveTopology, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    ShaderModuleDescriptor, ShaderSource, StoreOp, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor, VertexAttribute, VertexBufferLayout, VertexFormat, VertexState,
    VertexStepMode, util::DeviceExt,
};
use winit::dpi::PhysicalSize;

use crate::state::{ViewMode, ViewerState};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBUTES: [VertexAttribute; 2] = [
        VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: VertexFormat::Float32x3,
        },
        VertexAttribute {
            offset: core::mem::size_of::<[f32; 3]>() as u64,
            shader_location: 1,
            format: VertexFormat::Float32x3,
        },
    ];

    fn layout<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: core::mem::size_of::<Vertex>() as u64,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceRaw {
    model_0: [f32; 4],
    model_1: [f32; 4],
    model_2: [f32; 4],
    model_3: [f32; 4],
    tint: [f32; 4],
}

impl InstanceRaw {
    const ROW: u64 = core::mem::size_of::<[f32; 4]>() as u64;
    const ATTRIBUTES: [VertexAttribute; 5] = [
        VertexAttribute {
            offset: 0,
            shader_location: 2,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            offset: Self::ROW,
            shader_location: 3,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            offset: Self::ROW * 2,
            shader_location: 4,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            offset: Self::ROW * 3,
            shader_location: 5,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            offset: Self::ROW * 4,
            shader_location: 6,
            format: VertexFormat::Float32x4,
        },
    ];

    fn layout<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: core::mem::size_of::<InstanceRaw>() as u64,
            step_mode: VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

pub struct SceneStats {
    pub instance_count: usize,
}

pub struct Renderer {
    pipeline: RenderPipeline,
    camera_bind_group: BindGroup,
    camera_buffer: Buffer,
    vertex_buffer: Buffer,
    vertex_count: u32,
    instance_buffer: Buffer,
    instance_capacity: usize,
    depth: DepthTarget,
    size: PhysicalSize<u32>,
    clear_color: Color,
}

impl Renderer {
    pub fn new(device: &Device, format: TextureFormat, size: PhysicalSize<u32>) -> Self {
        let camera_uniform = CameraUniform {
            view_proj: camera_matrix(size),
        };
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("imu-viewer-wgpu-camera-buffer"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("imu-viewer-wgpu-camera-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("imu-viewer-wgpu-camera-bind-group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("imu-viewer-wgpu-shader"),
            source: ShaderSource::Wgsl(include_str!("../shader.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("imu-viewer-wgpu-pipeline-layout"),
            bind_group_layouts: &[&camera_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("imu-viewer-wgpu-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout(), InstanceRaw::layout()],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(format.into())],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DepthTarget::FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let mesh = imu_wire_mesh();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("imu-viewer-wgpu-mesh-buffer"),
            contents: bytemuck::cast_slice(&mesh),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let instance_capacity = 16;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("imu-viewer-wgpu-instance-buffer"),
            size: (instance_capacity * core::mem::size_of::<InstanceRaw>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            camera_bind_group,
            camera_buffer,
            vertex_buffer,
            vertex_count: mesh.len() as u32,
            instance_buffer,
            instance_capacity,
            depth: DepthTarget::new(device, size),
            size,
            clear_color: Color {
                r: 0.035,
                g: 0.04,
                b: 0.05,
                a: 1.0,
            },
        }
    }

    pub fn resize(&mut self, device: &Device, queue: &Queue, size: PhysicalSize<u32>) {
        self.size = size;
        self.depth = DepthTarget::new(device, size);
        queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: camera_matrix(size),
            }),
        );
    }

    pub fn render(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        view: &TextureView,
        state: &ViewerState,
    ) -> SceneStats {
        let instances = build_instances(state);
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("imu-viewer-wgpu-instance-buffer"),
                size: (self.instance_capacity * core::mem::size_of::<InstanceRaw>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }

        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("imu-viewer-wgpu-3d-pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.clear_color),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..instances.len() as u32);

        SceneStats {
            instance_count: instances.len(),
        }
    }
}

struct DepthTarget {
    view: TextureView,
}

impl DepthTarget {
    const FORMAT: TextureFormat = TextureFormat::Depth24Plus;

    fn new(device: &Device, size: PhysicalSize<u32>) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("imu-viewer-wgpu-depth"),
            size: wgpu::Extent3d {
                width: size.width.max(1),
                height: size.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        Self {
            view: texture.create_view(&TextureViewDescriptor::default()),
        }
    }
}

fn build_instances(state: &ViewerState) -> Vec<InstanceRaw> {
    let mut ids = if let Some(selected) = state.selected_imu {
        vec![selected]
    } else {
        state.sorted_imu_ids()
    };
    if ids.is_empty() {
        ids = (0..5)
            .map(|sensor_id| ImuId {
                system_id: 1,
                sensor_id,
            })
            .collect();
    }
    ids.into_iter()
        .enumerate()
        .map(|(index, imu_id)| {
            let position = if state.selected_imu.is_some() {
                [0.0, 0.0, 0.0]
            } else {
                grid_position(index)
            };
            let scale = if state.selected_imu.is_some() {
                1.8
            } else {
                1.0
            };
            let matrix = match state.view_mode {
                ViewMode::Quaternion => state
                    .interpolated_orientation
                    .get(&imu_id)
                    .map(|frame| {
                        model_matrix(position, scale, Some(frame.payload.quaternion), None)
                    })
                    .unwrap_or_else(|| model_matrix(position, scale, None, None)),
                ViewMode::Raw6Axis => {
                    let integrated = state.integrated_orientation.get(&imu_id).copied();
                    model_matrix(position, scale, None, integrated)
                }
            };
            InstanceRaw {
                model_0: matrix[0],
                model_1: matrix[1],
                model_2: matrix[2],
                model_3: matrix[3],
                tint: scene_color(index, state.selected_imu == Some(imu_id)),
            }
        })
        .collect()
}

fn imu_wire_mesh() -> Vec<Vertex> {
    let mut out = Vec::new();
    let p = [
        [-0.62, -0.38, -0.08],
        [0.62, -0.38, -0.08],
        [0.62, 0.38, -0.08],
        [-0.62, 0.38, -0.08],
        [-0.62, -0.38, 0.08],
        [0.62, -0.38, 0.08],
        [0.62, 0.38, 0.08],
        [-0.62, 0.38, 0.08],
    ];
    let edge_color = [0.75, 0.82, 0.92];
    for (a, b) in [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ] {
        push_line(&mut out, p[a], p[b], edge_color);
    }
    push_line(&mut out, [0.0, 0.0, 0.0], [0.92, 0.0, 0.0], [1.0, 0.2, 0.2]);
    push_line(
        &mut out,
        [0.0, 0.0, 0.0],
        [0.0, 0.92, 0.0],
        [0.2, 1.0, 0.35],
    );
    push_line(
        &mut out,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.92],
        [0.3, 0.55, 1.0],
    );
    out
}

fn push_line(out: &mut Vec<Vertex>, a: [f32; 3], b: [f32; 3], color: [f32; 3]) {
    out.push(Vertex { position: a, color });
    out.push(Vertex { position: b, color });
}

fn grid_position(index: usize) -> [f32; 3] {
    let col = index % 3;
    let row = index / 3;
    [(col as f32 - 1.0) * 2.1, 0.85 - row as f32 * 1.55, 0.0]
}

fn scene_color(index: usize, selected: bool) -> [f32; 4] {
    if selected {
        return [1.0, 0.92, 0.32, 1.0];
    }
    match index % 5 {
        0 => [0.45, 0.70, 0.98, 1.0],
        1 => [0.95, 0.52, 0.38, 1.0],
        2 => [0.36, 0.86, 0.52, 1.0],
        3 => [0.82, 0.48, 0.90, 1.0],
        _ => [0.92, 0.82, 0.40, 1.0],
    }
}

fn model_matrix(
    translation: [f32; 3],
    scale: f32,
    quaternion: Option<Quaternion>,
    integrated: Option<crate::state::IntegratedOrientation>,
) -> [[f32; 4]; 4] {
    let rotation = if let Some(q) = quaternion {
        quat_matrix(q)
    } else if let Some(state) = integrated {
        euler_matrix(state.roll, state.pitch, state.yaw)
    } else {
        identity_3()
    };

    [
        [
            rotation[0][0] * scale,
            rotation[0][1] * scale,
            rotation[0][2] * scale,
            0.0,
        ],
        [
            rotation[1][0] * scale,
            rotation[1][1] * scale,
            rotation[1][2] * scale,
            0.0,
        ],
        [
            rotation[2][0] * scale,
            rotation[2][1] * scale,
            rotation[2][2] * scale,
            0.0,
        ],
        [translation[0], translation[1], translation[2], 1.0],
    ]
}

fn quat_matrix(q: Quaternion) -> [[f32; 3]; 3] {
    let norm = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z)
        .sqrt()
        .max(1e-6);
    let w = q.w / norm;
    let x = q.x / norm;
    let y = q.y / norm;
    let z = q.z / norm;
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
        ],
        [
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
        ],
        [
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

fn euler_matrix(roll: f32, pitch: f32, yaw: f32) -> [[f32; 3]; 3] {
    let (sr, cr) = roll.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    [
        [cy * cp, cy * sp * sr - sy * cr, cy * sp * cr + sy * sr],
        [sy * cp, sy * sp * sr + cy * cr, sy * sp * cr - cy * sr],
        [-sp, cp * sr, cp * cr],
    ]
}

fn identity_3() -> [[f32; 3]; 3] {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
}

fn camera_matrix(size: PhysicalSize<u32>) -> [[f32; 4]; 4] {
    let aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
    let half_height = 2.4;
    let half_width = half_height * aspect;
    // WGSL matrix constructors take columns. This orthographic matrix keeps the
    // multi-IMU grid in view while we build out camera controls.
    [
        [1.0 / half_width, 0.0, 0.0, 0.0],
        [0.0, 1.0 / half_height, 0.0, 0.0],
        [0.0, 0.0, 0.02, 0.0],
        [0.0, 0.0, 0.5, 1.0],
    ]
}
