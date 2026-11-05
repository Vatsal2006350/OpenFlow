"""Overlay work-area conversion and bottom-center placement rules."""

cl def:pub overlay_position(facts: dict) -> dict {
    scale = float(facts.get("scale_factor", 1.0));
    if scale <= 0.0 { scale = 1.0; }
    area_x = float(facts["work_x"]);
    area_y = float(facts["monitor_y"]) + float(facts["work_y"]) / scale;
    area_w = float(facts["work_w"]) / scale;
    area_h = float(facts["work_h"]) / scale;
    window_w = float(facts["window_w"]);
    window_h = float(facts["window_h"]);
    bottom_inset = 12.0;
    x = area_x + (area_w - window_w) / 2.0;
    y = area_y + area_h - window_h - bottom_inset;
    max_x = max(area_x + area_w - window_w, area_x);
    max_y = max(area_y + area_h - window_h, area_y);
    x = min(max(x, area_x), max_x);
    y = min(max(y, area_y), max_y);
    return {"x": x, "y": y};
}
