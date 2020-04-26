use crate::*;

// This is required because the `vk::ShaderModuleCreateInfo` struct's `p_code`
// member expects a *u32, but `include_bytes!()` produces a Vec<u8>.
// TODO: Investigate how to properly address this.
#[allow(clippy::cast_ptr_alignment)]
fn create_shader_module(device: &ash::Device, code: Vec<u8>) -> vk::ShaderModule {
    let shader_module_create_info = vk::ShaderModuleCreateInfo {
        s_type: vk::StructureType::SHADER_MODULE_CREATE_INFO,
        p_next: ptr::null(),
        flags: vk::ShaderModuleCreateFlags::empty(),
        code_size: code.len(),
        p_code: code.as_ptr() as *const u32,
    };

    unsafe {
        device
            .create_shader_module(&shader_module_create_info, None)
            .expect("Failed to create shader module.")
    }
}

/// Returns Result<(shader modules, number of changed shader modules)
pub fn get_shader_modules(gpu: &Gpu) -> Option<(Vec<vk::ShaderModule>, usize)> {
    use std::path::Path;

    let glsl_files = ["default.vert", "default.frag"];

    // Ensure that shader cache directory exists
    std::fs::create_dir_all("_shadercache").expect("Couldn't create shader cache directory");

    // Build a list of all GLSL files which need to be compiled to SPIR-V
    let mut glsl_files_to_compile = Vec::new();
    for &file in glsl_files.iter() {
        let src_path_str = format!("assets/shaders/{}", file);
        let dst_path_str = format!("_shadercache/{}.spv", file);
        let src_path = Path::new(&src_path_str);
        let dst_path = Path::new(&dst_path_str);
        assert!(
            src_path.exists(),
            "Shader file assets/shaders/{} doesn't exist",
            file
        );
        let src_meta = src_path.metadata().expect(&format!(
            "Couldn't retrieve metadata for assets/shaders/{}",
            file
        ));
        if dst_path.exists() {
            if let Ok(dst_meta) = dst_path.metadata() {
                if let Ok(dst_modified) = dst_meta.modified() {
                    let src_modified = src_meta.modified().unwrap();
                    if dst_modified.duration_since(src_modified).is_ok() {
                        // Src was modified earlier than destination,
                        // i.e. no change
                        continue;
                    }
                }
            }
        }
        glsl_files_to_compile.push(file);
    }

    // Run glslc to compile GLSL shaders to SPIR-V
    let mut all_compilations_successful = true;
    for &file in glsl_files_to_compile.iter() {
        use std::process::Command;
        let glslc_output = Command::new("glslc")
            .arg(&format!("assets/shaders/{}", file))
            .arg("-o")
            .arg(&format!("_shadercache/{}.spv", file))
            .output()
            .expect("`glslc`, the GLSL -> SPIR-V compiler, could not be invoked.");
        if !glslc_output.status.success() {
            println!(
                "{}:\n    {}",
                file,
                String::from_utf8(glslc_output.stderr).unwrap()
            );
            all_compilations_successful = false;
        }
    }

    if !all_compilations_successful {
        return None;
    }

    // Load shaders from the SPIR-V files
    let modules = glsl_files
        .iter()
        .map(|&glsl_file| {
            let spirv_file = &format!("_shadercache/{}.spv", glsl_file);
            create_shader_module(
                &gpu.device,
                std::fs::read(spirv_file)
                    .expect(&format!("Failed to read shader file {}", spirv_file)),
            )
        })
        .collect();

    Some((modules, glsl_files_to_compile.len()))
}