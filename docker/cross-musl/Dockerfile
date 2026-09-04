# Cross image for the musl release targets (x86_64-unknown-linux-musl and
# aarch64-unknown-linux-musl).
#
# The vendored bubblewrap (ADR-029 §3) links libcap, and the musl release
# binary must be fully static. The default cross musl images are Ubuntu-based
# with a musl-cross toolchain but no musl libcap: the distro `libcap-dev` is
# glibc-linked and unusable for a static musl build. So this image builds a
# static musl `libcap.a` from source and installs it (plus a `libcap.pc`) where
# `pkg-config` can find it.
#
# `CROSS_BASE_IMAGE` is provided by cross and points at the default image for
# the target, so this single Dockerfile serves both musl targets.

ARG CROSS_BASE_IMAGE
FROM $CROSS_BASE_IMAGE

ARG LIBCAP_VERSION=2.69
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates make gcc \
    && curl -sL -o /tmp/libcap.tar.gz \
        "https://www.kernel.org/pub/linux/libs/security/linux-privs/libcap2/libcap-${LIBCAP_VERSION}.tar.gz" \
    && tar -xzf /tmp/libcap.tar.gz -C /tmp \
    && cd /tmp/libcap-${LIBCAP_VERSION} \
    && MUSL_GCC="$(basename "$(ls /usr/local/bin/*-musl-gcc 2>/dev/null | head -1)")" \
    && MUSL_OBJCOPY="$(basename "$(ls /usr/local/bin/*-musl-objcopy 2>/dev/null | head -1)")" \
    && make CC="$MUSL_GCC" OBJCOPY="$MUSL_OBJCOPY" BUILD_CC=gcc lib=lib RAISE_SETFCAP=no PAM_CAP=no \
    && install -D libcap/libcap.a /usr/local/lib/libcap.a \
    && install -D libcap/include/sys/capability.h /usr/local/include/sys/capability.h \
    && install -D libcap/include/uapi/linux/capability.h /usr/local/include/uapi/linux/capability.h \
    && mkdir -p /usr/local/lib/pkgconfig \
    && printf 'prefix=/usr/local\nexec_prefix=${prefix}\nlibdir=${exec_prefix}/lib\nincludedir=${prefix}/include\n\nName: libcap\nDescription: Linux capabilities library\nVersion: %s\nLibs: -L${libdir} -lcap\nCflags: -I${includedir}\n' "${LIBCAP_VERSION}" \
        > /usr/local/lib/pkgconfig/libcap.pc \
    && rm -rf /tmp/libcap*
