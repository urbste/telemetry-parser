"""Small pyTheiaSfM camera loader.

Mirrors the `Camera` pattern used in the consumer app: load intrinsics from
the JSON emitted by fit_division_model.py (or a hand-written PINHOLE /
FISHEYE target), optionally rescale, and expose the underlying Theia
CameraIntrinsics for ImageToCameraCoordinates / CameraToImageCoordinates.

Supported intrinsic_type values (extend as needed to match your backend):
    DIVISION_UNDISTORTION   (5 params: f, aspect, cx, cy, k)
    PINHOLE                 (4 params: f, aspect, cx, cy)
    FISHEYE                 (8 params: f, aspect, cx, cy, k1..k4)
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

# NOTE: pytheia must be imported BEFORE numpy on some installations
# (e.g. anaconda on Linux). numpy pulls in an older libstdc++ which then
# prevents pytheia's C++ extension from finding the GLIBCXX symbols it needs.
import pytheia as pt
import numpy as np  # noqa: F401, E402


class TheiaCamera:
    """Lightweight camera loader bound to pyTheiaSfM."""

    def __init__(self) -> None:
        self.intr: pt.sfm.Camera | None = None
        self.cam_intr_json: dict[str, Any] | None = None
        self.prior: pt.sfm.CameraIntrinsicsPrior | None = None

    def get_camera(self) -> pt.sfm.Camera:
        assert self.intr is not None, "Call load_* first."
        return self.intr

    def load_from_dict(self, cam_json: dict[str, Any], scale: float = 1.0) -> None:
        self.cam_intr_json = cam_json
        self._materialise(scale)

    def load_from_file(self, path: str | Path, scale: float = 1.0) -> None:
        self.cam_intr_json = json.loads(Path(path).read_text(encoding="utf-8"))
        self._materialise(scale)

    def _materialise(self, scale: float) -> None:
        assert self.cam_intr_json is not None

        self.intr = pt.sfm.Camera()
        self.prior = pt.sfm.CameraIntrinsicsPrior()

        ins = self.cam_intr_json["intrinsics"]
        self.prior.aspect_ratio.value = [float(ins.get("aspect_ratio", 1.0))]
        self.prior.image_width = int(round(self.cam_intr_json["image_width"] * scale))
        self.prior.image_height = int(round(self.cam_intr_json["image_height"] * scale))
        self.prior.principal_point.value = [
            float(ins["principal_pt_x"]) * scale,
            float(ins["principal_pt_y"]) * scale,
        ]
        self.prior.focal_length.value = [float(ins["focal_length"]) * scale]
        self.prior.skew.value = [float(ins.get("skew", 0.0))]
        self.prior.camera_intrinsics_model_type = self.cam_intr_json["intrinsic_type"]
        self._apply_distortion(scale)

        self.intr.SetFromCameraIntrinsicsPriors(self.prior)

    def _apply_distortion(self, scale: float) -> None:
        assert self.prior is not None and self.cam_intr_json is not None
        model = self.prior.camera_intrinsics_model_type
        ins = self.cam_intr_json["intrinsics"]

        if model == "DIVISION_UNDISTORTION":
            # k has units of 1/px^2 -> rescales by 1/scale^2
            k = float(ins["div_undist_distortion"]) / (scale * scale)
            self.prior.radial_distortion.value = [k, 0.0, 0.0, 0.0]
        elif model == "FISHEYE":
            self.prior.radial_distortion.value = [
                float(ins["radial_distortion_1"]),
                float(ins["radial_distortion_2"]),
                float(ins["radial_distortion_3"]),
                float(ins["radial_distortion_4"]),
            ]
        elif model == "PINHOLE":
            # No distortion fields.
            pass
        else:
            raise ValueError(f"Unsupported intrinsic_type: {model!r}")

    @property
    def width(self) -> int:
        assert self.prior is not None
        return int(self.prior.image_width)

    @property
    def height(self) -> int:
        assert self.prior is not None
        return int(self.prior.image_height)

    def fov_x_deg(self) -> float:
        assert self.prior is not None
        import math

        f = float(self.prior.focal_length.value[0])
        return math.degrees(2.0 * math.atan2(self.width * 0.5, f))
