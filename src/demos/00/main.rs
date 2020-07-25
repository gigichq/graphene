use ash::version::DeviceV1_0;
use ash::vk;
use glam::*;
use std::f32::consts::PI;

const DEGREES_TO_RADIANS: f32 = PI / 180.0;

#[allow(dead_code)]
struct UniformBuffer {
    mtx_obj_to_clip: Mat4,
    mtx_norm_obj_to_world: Mat4,
    elapsed_seconds: f32,
}

fn execute_pass(
    ctx: &mut graphene::Context,
    elapsed_seconds: f32,
    uniform_buffer: graphene::BufferHandle,
    cmd_buf: vk::CommandBuffer,
    mesh: &graphene::Mesh,
) {
    // Update uniform buffer
    {
        let cam_pos = Vec3::new(0.0, -6.0, 0.0);
        let cam_rot = Quat::from_rotation_z((elapsed_seconds * 1.5).sin() * 0.125 * PI);
        let obj_pos = Vec3::new(0.0, 0.0, 0.0);
        let obj_rot = Quat::from_rotation_z(elapsed_seconds * 0.3);
        let obj_scale = Vec3::new(1.0, 1.0, 1.0);

        let mtx_rot_scale = Mat4::from_quat(obj_rot)
            * Mat4::from_scale(obj_scale)
            * Mat4::from_rotation_x(90.0 * DEGREES_TO_RADIANS);
        let mtx_obj_to_world = Mat4::from_rotation_x(90.0 * DEGREES_TO_RADIANS)
            * Mat4::from_translation(obj_pos)
            * mtx_rot_scale;
        let mtx_world_to_view = Mat4::from_rotation_x(90.0 * DEGREES_TO_RADIANS)
            * Mat4::from_quat(cam_rot)
            * Mat4::from_translation(-cam_pos)
            * Mat4::from_rotation_x(-90.0 * DEGREES_TO_RADIANS);
        let mtx_view_to_clip = {
            let width = ctx.facade.swapchain_width;
            let height = ctx.facade.swapchain_height;
            Mat4::perspective_lh(
                60.0 * DEGREES_TO_RADIANS,
                width as f32 / height as f32,
                0.01,
                100.0,
            )
        };

        /* This matrix is an orthogonal matrix if scaling is uniform, in
        which case the inverse transpose is the same as the matrix itself.
        // Pass 0
        However, we want to support non-uniform scaling, so we
        do the inverse transpose. */
        let mtx_norm_obj_to_world = mtx_rot_scale.inverse().transpose();

        let ubos = [UniformBuffer {
            mtx_obj_to_clip: mtx_view_to_clip * mtx_world_to_view * mtx_obj_to_world,
            mtx_norm_obj_to_world,
            elapsed_seconds,
        }];

        ctx.upload_data(uniform_buffer, &ubos);
    }
    // Bind index and vertex buffers
    unsafe {
        {
            let vertex_buffers = [mesh.vertex_buffer.vk_buffer];
            let offsets = [0_u64];
            ctx.gpu
                .device
                .cmd_bind_vertex_buffers(cmd_buf, 0, &vertex_buffers, &offsets);
            ctx.gpu.device.cmd_bind_index_buffer(
                cmd_buf,
                mesh.index_buffer.vk_buffer,
                0,
                vk::IndexType::UINT32,
            );
        }

        ctx.gpu
            .device
            .cmd_draw_indexed(cmd_buf, mesh.index_buffer.num_elements as u32, 1, 0, 0, 0);
    }
}

fn main() {
    let mut ctx = graphene::Context::new();
    let start_instant = std::time::Instant::now();

    let mesh = graphene::Mesh::load("assets/meshes/suzanne.glb", &ctx.gpu, ctx.command_pool);
    let mesh2 = graphene::Mesh::load("assets/meshes/sphere.glb", &ctx.gpu, ctx.command_pool);
    let depth_texture = ctx
        .new_texture_relative_size(
            "depth",
            1.0,
            vk::Format::D32_SFLOAT,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH,
        )
        .unwrap();
    let temp_texture = ctx
        .new_texture_relative_size(
            "temp",
            1.0,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::COLOR_ATTACHMENT,
            vk::ImageAspectFlags::COLOR,
        )
        .unwrap();
    let environment_sampler = graphene::Sampler::new(&ctx.gpu);
    let environment_texture = ctx
        .new_texture_from_file(
            "environment map",
            "assets/textures/env_carpentry_shop_02_2k.jpg",
        )
        .unwrap();

    // TODO: Remove this and implement a shader API
    let shader_modules = vec![
        vec![ctx.shader_modules[0], ctx.shader_modules[1]],
        vec![ctx.shader_modules[0], ctx.shader_modules[2]],
    ];

    loop {
        if !ctx.begin_frame() {
            break;
        }

        let elapsed_seconds = start_instant.elapsed().as_secs_f32();
        let cmd_buf = ctx.command_buffers[ctx.swapchain_idx];

        // Build and execute render graph
        let mut graph_builder = graphene::GraphBuilder::new();
        let uniform_buffer = ctx
            .new_buffer(
                // TODO: Avoid having the swapchain index, automatically
                // creating a unique uniform buffer per pass and per graph
                &format!("uniform buffer_{}", ctx.swapchain_idx),
                std::mem::size_of::<UniformBuffer>(),
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )
            .unwrap();
        let pass_0 = ctx
            .add_pass(
                &mut graph_builder,
                "forward lit",
                &vec![temp_texture],
                Some(depth_texture),
                &shader_modules[0],
                uniform_buffer,
                environment_texture,
                &environment_sampler,
            )
            .unwrap();
        let pass_1 = ctx
            .add_pass(
                &mut graph_builder,
                "forward lit 2",
                &vec![ctx.facade.swapchain_textures[ctx.swapchain_idx]],
                Some(depth_texture),
                &shader_modules[1],
                uniform_buffer,
                environment_texture,
                &environment_sampler,
            )
            .unwrap();

        let graph = ctx.build_graph(graph_builder);
        // Pass 0
        ctx.begin_pass(graph, pass_0);
        execute_pass(&mut ctx, elapsed_seconds, uniform_buffer, cmd_buf, &mesh);
        ctx.end_pass(graph);
        // Pass 1
        ctx.begin_pass(graph, pass_1);
        execute_pass(&mut ctx, elapsed_seconds, uniform_buffer, cmd_buf, &mesh2);
        ctx.end_pass(graph);

        ctx.end_frame();
    }

    // TODO: Remove the necessity for this sync
    unsafe {
        ctx.gpu
            .device
            .device_wait_idle()
            .expect("Failed to wait device idle!");
    }
}
