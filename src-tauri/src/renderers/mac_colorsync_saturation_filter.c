#include <ApplicationServices/ApplicationServices.h>
#include <ColorSync/ColorSync.h>
#include <CoreFoundation/CoreFoundation.h>
#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint32_t be32(const uint8_t *p) {
    return ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) |
           ((uint32_t)p[2] << 8) | (uint32_t)p[3];
}

static void put_be32(uint8_t *p, uint32_t v) {
    p[0] = (uint8_t)(v >> 24);
    p[1] = (uint8_t)(v >> 16);
    p[2] = (uint8_t)(v >> 8);
    p[3] = (uint8_t)v;
}

static double s15_fixed16(const uint8_t *p) {
    return (double)(int32_t)be32(p) / 65536.0;
}

static void put_s15_fixed16(uint8_t *p, double x) {
    int32_t v = (int32_t)llround(x * 65536.0);
    put_be32(p, (uint32_t)v);
}

static CFStringRef tag_signature(const char *value) {
    return CFStringCreateWithCString(
        kCFAllocatorDefault,
        value,
        kCFStringEncodingASCII
    );
}

static bool copy_xyz_tag(ColorSyncProfileRef profile, const char *tag, double out[3]) {
    CFStringRef key = tag_signature(tag);
    CFDataRef data = ColorSyncProfileCopyTag(profile, key);
    CFRelease(key);
    if (!data) {
        return false;
    }

    const uint8_t *bytes = CFDataGetBytePtr(data);
    CFIndex len = CFDataGetLength(data);
    if (len < 20 || memcmp(bytes, "XYZ ", 4) != 0) {
        CFRelease(data);
        return false;
    }

    out[0] = s15_fixed16(bytes + 8);
    out[1] = s15_fixed16(bytes + 12);
    out[2] = s15_fixed16(bytes + 16);
    CFRelease(data);
    return true;
}

static CFDataRef create_xyz_tag(const double value[3]) {
    uint8_t bytes[20] = {0};
    memcpy(bytes, "XYZ ", 4);
    put_s15_fixed16(bytes + 8, value[0]);
    put_s15_fixed16(bytes + 12, value[1]);
    put_s15_fixed16(bytes + 16, value[2]);
    return CFDataCreate(kCFAllocatorDefault, bytes, sizeof(bytes));
}

static bool set_xyz_tag(ColorSyncMutableProfileRef profile, const char *tag, const double value[3]) {
    CFStringRef key = tag_signature(tag);
    CFDataRef data = create_xyz_tag(value);
    ColorSyncProfileSetTag(profile, key, data);
    CFRelease(data);
    CFRelease(key);
    return true;
}

static bool inverse3(const double matrix[3][3], double out[3][3]) {
    double det =
        matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1]) -
        matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0]) +
        matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]);
    if (fabs(det) < 1e-9) {
        return false;
    }
    double inv_det = 1.0 / det;
    out[0][0] =  (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1]) * inv_det;
    out[0][1] = -(matrix[0][1] * matrix[2][2] - matrix[0][2] * matrix[2][1]) * inv_det;
    out[0][2] =  (matrix[0][1] * matrix[1][2] - matrix[0][2] * matrix[1][1]) * inv_det;
    out[1][0] = -(matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0]) * inv_det;
    out[1][1] =  (matrix[0][0] * matrix[2][2] - matrix[0][2] * matrix[2][0]) * inv_det;
    out[1][2] = -(matrix[0][0] * matrix[1][2] - matrix[0][2] * matrix[1][0]) * inv_det;
    out[2][0] =  (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]) * inv_det;
    out[2][1] = -(matrix[0][0] * matrix[2][1] - matrix[0][1] * matrix[2][0]) * inv_det;
    out[2][2] =  (matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0]) * inv_det;
    return true;
}

static void multiply3(const double a[3][3], const double b[3][3], double out[3][3]) {
    memset(out, 0, sizeof(double) * 9);
    for (int row = 0; row < 3; row++) {
        for (int col = 0; col < 3; col++) {
            for (int k = 0; k < 3; k++) {
                out[row][col] += a[row][k] * b[k][col];
            }
        }
    }
}

static bool write_profile(ColorSyncMutableProfileRef profile, const char *path) {
    CFErrorRef error = NULL;
    CFDataRef data = ColorSyncProfileCopyData(profile, &error);
    if (!data) {
        if (error) {
            CFShow(error);
            CFRelease(error);
        }
        return false;
    }

    FILE *file = fopen(path, "wb");
    if (!file) {
        CFRelease(data);
        return false;
    }
    fwrite(CFDataGetBytePtr(data), 1, (size_t)CFDataGetLength(data), file);
    fclose(file);
    CFRelease(data);
    return true;
}

static bool copy_current_display_profile(const char *path) {
    ColorSyncProfileRef profile = ColorSyncProfileCreateWithDisplayID(CGMainDisplayID());
    if (!profile) {
        return false;
    }
    CFErrorRef error = NULL;
    CFDataRef data = ColorSyncProfileCopyData(profile, &error);
    CFRelease(profile);
    if (!data) {
        if (error) {
            CFShow(error);
            CFRelease(error);
        }
        return false;
    }

    FILE *file = fopen(path, "wb");
    if (!file) {
        CFRelease(data);
        return false;
    }
    fwrite(CFDataGetBytePtr(data), 1, (size_t)CFDataGetLength(data), file);
    fclose(file);
    CFRelease(data);
    return true;
}

static bool apply_profile(const char *path) {
    CGDirectDisplayID display = CGMainDisplayID();
    CFUUIDRef uuid = CGDisplayCreateUUIDFromDisplayID(display);
    if (!uuid) {
        return false;
    }

    CFStringRef path_string = CFStringCreateWithCString(
        kCFAllocatorDefault,
        path,
        kCFStringEncodingUTF8
    );
    CFURLRef url = CFURLCreateWithFileSystemPath(
        kCFAllocatorDefault,
        path_string,
        kCFURLPOSIXPathStyle,
        false
    );
    CFMutableDictionaryRef profiles = CFDictionaryCreateMutable(
        kCFAllocatorDefault,
        0,
        &kCFTypeDictionaryKeyCallBacks,
        &kCFTypeDictionaryValueCallBacks
    );
    CFDictionarySetValue(profiles, kColorSyncDeviceDefaultProfileID, url);
    bool ok = ColorSyncDeviceSetCustomProfiles(
        kColorSyncDisplayDeviceClass,
        uuid,
        profiles
    );

    CFRelease(profiles);
    CFRelease(url);
    CFRelease(path_string);
    CFRelease(uuid);
    return ok;
}

bool softwane_macos_colorsync_reset_saturation(void) {
    CGDirectDisplayID display = CGMainDisplayID();
    CFUUIDRef uuid = CGDisplayCreateUUIDFromDisplayID(display);
    if (!uuid) {
        return false;
    }

    CFMutableDictionaryRef profiles = CFDictionaryCreateMutable(
        kCFAllocatorDefault,
        0,
        &kCFTypeDictionaryKeyCallBacks,
        &kCFTypeDictionaryValueCallBacks
    );
    CFDictionarySetValue(profiles, kColorSyncDeviceDefaultProfileID, kCFNull);
    bool ok = ColorSyncDeviceSetCustomProfiles(
        kColorSyncDisplayDeviceClass,
        uuid,
        profiles
    );

    CFRelease(profiles);
    CFRelease(uuid);
    return ok;
}

bool softwane_macos_colorsync_capture_baseline(const char *baseline_profile_path) {
    return copy_current_display_profile(baseline_profile_path);
}

bool softwane_macos_colorsync_set_saturation(
    double saturation,
    const char *baseline_profile_path,
    const char *profile_path
) {
    saturation = fmax(0.2, fmin(1.0, saturation));

    CFStringRef baseline_path_string = CFStringCreateWithCString(
        kCFAllocatorDefault,
        baseline_profile_path,
        kCFStringEncodingUTF8
    );
    CFURLRef baseline_url = CFURLCreateWithFileSystemPath(
        kCFAllocatorDefault,
        baseline_path_string,
        kCFURLPOSIXPathStyle,
        false
    );
    CFErrorRef baseline_error = NULL;
    ColorSyncProfileRef base = ColorSyncProfileCreateWithURL(baseline_url, &baseline_error);
    CFRelease(baseline_url);
    CFRelease(baseline_path_string);
    if (baseline_error) {
        CFShow(baseline_error);
        CFRelease(baseline_error);
    }
    if (!base) {
        return false;
    }
    ColorSyncMutableProfileRef profile = ColorSyncProfileCreateMutableCopy(base);
    if (!profile) {
        CFRelease(base);
        return false;
    }

    double red[3], green[3], blue[3];
    bool ok = copy_xyz_tag(base, "rXYZ", red) &&
              copy_xyz_tag(base, "gXYZ", green) &&
              copy_xyz_tag(base, "bXYZ", blue);
    CFRelease(base);
    if (!ok) {
        CFRelease(profile);
        return false;
    }

    double display_matrix[3][3] = {
        {red[0], green[0], blue[0]},
        {red[1], green[1], blue[1]},
        {red[2], green[2], blue[2]},
    };
    const double luma[3] = {0.2126, 0.7152, 0.0722};
    double saturation_matrix[3][3];
    for (int row = 0; row < 3; row++) {
        for (int col = 0; col < 3; col++) {
            saturation_matrix[row][col] =
                luma[col] * (1.0 - saturation) + (row == col ? saturation : 0.0);
        }
    }

    double inverse_saturation_matrix[3][3];
    if (!inverse3(saturation_matrix, inverse_saturation_matrix)) {
        CFRelease(profile);
        return false;
    }
    double result[3][3];
    multiply3(display_matrix, inverse_saturation_matrix, result);

    double new_red[3] = {result[0][0], result[1][0], result[2][0]};
    double new_green[3] = {result[0][1], result[1][1], result[2][1]};
    double new_blue[3] = {result[0][2], result[1][2], result[2][2]};
    set_xyz_tag(profile, "rXYZ", new_red);
    set_xyz_tag(profile, "gXYZ", new_green);
    set_xyz_tag(profile, "bXYZ", new_blue);

    CFErrorRef errors = NULL;
    CFErrorRef warnings = NULL;
    ok = ColorSyncProfileVerify(profile, &errors, &warnings);
    if (errors) {
        CFShow(errors);
        CFRelease(errors);
    }
    if (warnings) {
        CFShow(warnings);
        CFRelease(warnings);
    }
    if (!ok) {
        CFRelease(profile);
        return false;
    }

    ok = write_profile(profile, profile_path) && apply_profile(profile_path);
    CFRelease(profile);
    return ok;
}
