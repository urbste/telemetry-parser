#ifndef TELEMETRY_PARSER_H
#define TELEMETRY_PARSER_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Open a video file. Returns an opaque handle, or 0 on failure.
uint64_t telemetry_parser_open(const char *path);

/// Release resources associated with a handle.
void telemetry_parser_close(uint64_t handle);

/// Free a string returned by this library.
void telemetry_parser_free_string(char *value);

/// Last error message from the most recent failing call, or null.
char *telemetry_parser_last_error(void);

/// Camera family string (caller must free with telemetry_parser_free_string).
char *telemetry_parser_get_camera_type(uint64_t handle);

/// Clip duration in seconds.
float telemetry_parser_get_duration(uint64_t handle);

/// JSON object with frameCount, fpsNumerator, fpsDenominator; null if unavailable.
char *telemetry_parser_get_video_info_json(uint64_t handle);

/// DVID/FOVL lens JSON when present; null otherwise.
char *telemetry_parser_get_lens_metadata_json(uint64_t handle);

/// JSON array of GPS points.
char *telemetry_parser_get_gps_json(uint64_t handle);

/// JSON array of accelerometer samples.
char *telemetry_parser_get_accelerometer_json(uint64_t handle);

/// JSON array of gyroscope samples.
char *telemetry_parser_get_gyroscope_json(uint64_t handle);

/// JSON array of gravity vector samples.
char *telemetry_parser_get_gravity_json(uint64_t handle);

/// JSON array of CORI orientation samples.
char *telemetry_parser_get_camera_orientation_json(uint64_t handle);

/// JSON array of IORI orientation samples.
char *telemetry_parser_get_image_orientation_json(uint64_t handle);

/// JSON array of combined orientation samples.
char *telemetry_parser_get_orientation_combined_json(uint64_t handle);

#ifdef __cplusplus
}
#endif

#endif /* TELEMETRY_PARSER_H */
