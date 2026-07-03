#include "sense_voice_capi.h"
#include "common.h"
#include "sense-voice.h"
#include <cstring>
#include <vector>

struct SenseVoiceHandle {
    sense_voice_context* ctx;
};

extern "C" {

void* aura_sense_voice_load(const char* model_path, int use_gpu) {
    sense_voice_context_params params = sense_voice_context_default_params();
    params.use_gpu = use_gpu;
    params.flash_attn = false;
    params.use_itn = true;

    sense_voice_context* ctx = sense_voice_small_init_from_file_with_params(
        model_path, params);
    if (!ctx) return nullptr;

    ctx->language_id = sense_voice_lang_id("auto");

    auto* handle = new SenseVoiceHandle{ctx};
    return handle;
}

int aura_sense_voice_transcribe(void* handle_ptr, const float* pcm_data,
                                 int num_samples, char* out_text,
                                 int max_text_len) {
    auto* handle = static_cast<SenseVoiceHandle*>(handle_ptr);
    if (!handle || !handle->ctx) return -1;

    // Convert f32 PCM to f64 for SenseVoice.cpp API
    std::vector<double> pcmf32(pcm_data, pcm_data + num_samples);

    sense_voice_full_params wparams = sense_voice_full_default_params(
        SENSE_VOICE_SAMPLING_GREEDY);
    wparams.n_threads = 4;
    wparams.language = "auto";

    handle->ctx->state->duration =
        float(num_samples) / SENSE_VOICE_SAMPLE_RATE;

    int ret = sense_voice_full_parallel(handle->ctx, wparams, pcmf32,
                                        num_samples, 1);
    if (ret != 0) return ret;

    // Extract text from token IDs (same logic as sense_voice_print_output)
    std::string result;
    for (size_t i = 4; i < handle->ctx->state->ids.size(); i++) {
        int id = handle->ctx->state->ids[i];
        if (i > 0 && handle->ctx->state->ids[i - 1] == id) continue;
        if (id > 0) {
            result += handle->ctx->vocab.id_to_token[id];
        }
    }

    // Reset state for next call
    handle->ctx->state->ids.clear();

    strncpy(out_text, result.c_str(), max_text_len - 1);
    out_text[max_text_len - 1] = '\0';
    return 0;
}

void aura_sense_voice_free(void* handle_ptr) {
    auto* handle = static_cast<SenseVoiceHandle*>(handle_ptr);
    if (!handle) return;

    if (handle->ctx) {
        sense_voice_context* ctx = handle->ctx;

        ggml_free(ctx->model.ctx);
        ggml_backend_buffer_free(ctx->model.buffer);
        ggml_backend_buffer_free(ctx->vad_model.buffer);

        sense_voice_free_state(ctx->state);

        delete ctx->model.model->encoder;
        delete ctx->model.model;
        delete ctx->vad_model.model;
        delete ctx;
    }

    delete handle;
}

}
