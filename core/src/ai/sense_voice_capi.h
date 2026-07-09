#ifndef AURA_SENSE_VOICE_CAPI_H
#define AURA_SENSE_VOICE_CAPI_H

#ifdef __cplusplus
extern "C" {
#endif

void* aura_sense_voice_load(const char* model_path, int use_gpu);
int aura_sense_voice_transcribe(void* ctx, const float* pcm_data,
                                int num_samples, char* out_text,
                                int max_text_len, int n_threads);
void aura_sense_voice_free(void* ctx);

#ifdef __cplusplus
}
#endif

#endif
