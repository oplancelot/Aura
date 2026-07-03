use std::ffi::c_char;
use std::os::raw::c_int;

extern "C" {
    pub fn aura_sense_voice_load(
        model_path: *const c_char,
        use_gpu: c_int,
    ) -> *mut std::ffi::c_void;

    pub fn aura_sense_voice_transcribe(
        ctx: *mut std::ffi::c_void,
        pcm_data: *const f32,
        num_samples: c_int,
        out_text: *mut c_char,
        max_text_len: c_int,
    ) -> c_int;

    pub fn aura_sense_voice_free(ctx: *mut std::ffi::c_void);
}
