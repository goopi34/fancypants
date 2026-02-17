// Belt-mounted wearable enclosure system for VL53L0X sensor + Feather 32u4
// Two-part design: Front sensor enclosure + Hip controller enclosure
// Optimized for resin (DLP) printing

// ============================================================================
// GLOBAL PARAMETERS
// ============================================================================

$fn = 50;

// Common parameters
wall = 4;               // Wall thickness (resin-optimized)
clearance = 1.5;        // Component clearance
belt_width = 43;        // Belt width
belt_thickness = 7;     // Belt thickness at head
loop_clearance = 1;     // Extra clearance for belt in slots

// Screw boss parameters (3mm wood screws)
boss_od = 8;            // Outer diameter of screw boss (larger for ribbed interior)
boss_hole = 3.4;        // Oversized hole in boss (screw doesn't engage wall)
boss_rib_id = 2.6;      // Inner diameter at rib tips (screw bites into ribs)
boss_rib_count = 4;     // Number of grip ribs
boss_rib_width = 1.2;   // Angular width of each rib (mm at hole wall)
boss_clearance = 3.4;   // Clearance hole in lid for screw shaft
boss_counterbore_d = 6; // Counterbore diameter for screw head
boss_counterbore_depth = 2; // Counterbore depth in lid top

// Rounding
edge_r = 2;             // Fillet/round radius for edges and corners

// Belt tab parameters
tab_thickness = 5;      // Tab slab thickness
tab_wall = 5;           // Material bridge width flanking belt slot
tab_r = 1;              // Tab bottom edge rounding radius (kept small for printability)

// ============================================================================
// UTILITY MODULES
// ============================================================================

// Rounded rectangle 2D profile
module rounded_rect_2d(w, h, r) {
    offset(r) offset(-r) square([w, h], center=true);
}

// Box with rounded top edges only, perfectly flat bottom (for lids)
module rounded_box(w, h, d, r) {
    actual_r = min(r, d * 0.45);  // Clamp so it works for thin lids
    hull() {
        // Bottom: full-size rounded-corner rectangle, thin slab
        linear_extrude(height=0.01)
            rounded_rect_2d(w, h, actual_r);
        // Just below the top rounding: full-size
        translate([0, 0, d - actual_r])
            linear_extrude(height=0.01)
                rounded_rect_2d(w, h, actual_r);
        // Top: inset by actual_r on all sides, at full height
        translate([0, 0, d - 0.01])
            linear_extrude(height=0.01)
                rounded_rect_2d(w - 2*actual_r, h - 2*actual_r, 0.01);
    }
}

// Box with rounded vertical edges, flat top (for enclosure bodies)
module rounded_box_flat_top(w, h, d, r) {
    linear_extrude(height=d)
        rounded_rect_2d(w, h, r);
}

// Ribbed screw boss hole - oversized bore with inward ribs for thread grip
// Use in difference() to cut into a solid boss cylinder.
// Creates a hole_d bore with rib_count ribs protruding inward to rib_id.
module ribbed_boss_hole(hole_d, rib_id, rib_width, rib_count, h) {
    difference() {
        // Main oversized bore
        cylinder(d=hole_d, h=h);
        // Leave behind the ribs by subtracting the bore minus rib volumes
        // i.e., don't cut where the ribs are
        for(i = [0 : rib_count - 1]) {
            rotate([0, 0, i * (360 / rib_count)])
                translate([0, 0, -0.5])
                    linear_extrude(height=h + 1)
                        translate([rib_id/2, -rib_width/2])
                            square([(hole_d - rib_id)/2 + 0.1, rib_width]);
        }
    }
}

// Belt tab: rectangular body with ALL bottom edges and outer corners rounded
// Belt slot inner vertical edges also rounded for skin comfort
// Oriented: extends along +Y from origin, width along X
module belt_tab(tab_w, tab_l, tab_t, slot_w, slot_h, r) {
    difference() {
        // Start with a fully rounded shape via minkowski
        translate([0, tab_l/2, 0])
            minkowski() {
                translate([0, 0, tab_t/2])
                    cube([tab_w - 2*r, tab_l, tab_t - 2*r], center=true);
                sphere(r=r);
            }
        
        // Chop off the top dome - flatten to tab_t height
        translate([0, tab_l/2, tab_t + r + 0.5])
            cube([tab_w + 2*r + 1, tab_l + 2*r + 1, 2*r + 1], center=true);
        
        // Chop off everything below Z=0
        translate([0, tab_l/2, -(r + 0.5)])
            cube([tab_w + 2*r + 1, tab_l + 2*r + 1, 2*r + 1], center=true);
        
        // Belt slot - hull of 4 rounded-corner cylinders for rounded vertical edges
        hull() {
            for(x = [-1, 1]) {
                for(y = [-1, 1]) {
                    translate([x * (slot_w/2 - r), tab_l/2 + y * (slot_h/2 - r), -1])
                        cylinder(r=r, h=tab_t + 4);
                }
            }
        }
    }
}

// ============================================================================
// FRONT ENCLOSURE - VL53L0X SENSOR
// ============================================================================

module front_enclosure_bottom() {
    
    // VL53L0X parameters
    sensor_pcb_w = 25;
    sensor_pcb_h = 17;
    sensor_pcb_t = 3;
    sensor_component_height = 2;
    sensor_total_height = sensor_pcb_t + sensor_component_height;
    
    // Sensor lens
    lens_w = 4;
    lens_h = 5;
    
    // Mounting holes
    hole_offset = 3;
    
    // JST connector
    jst_housing_w = 8;
    jst_housing_h = 3.5;
    jst_cable_w = 4;
    jst_cable_h = 1.5;
    
    // Enclosure dimensions
    enclosure_w = sensor_pcb_w + 2 * (clearance + wall);
    enclosure_h = sensor_pcb_h + 2 * (clearance + wall);
    enclosure_d = sensor_total_height + clearance + wall;
    
    // Belt tab dimensions
    belt_slot_w = belt_width + 2 * loop_clearance;
    belt_slot_h = belt_thickness + 2 * loop_clearance;
    tab_total_w = belt_slot_w + 2 * tab_wall;
    tab_total_l = belt_slot_h + 2 * tab_wall;
    
    // Screw boss positions - inside wall with 1.5mm margin to exterior
    boss_x = enclosure_w/2 - boss_od/2 + 0.5;
    boss_y = enclosure_h/2 - boss_od/2 + 0.5;
    
    difference() {
        union() {
            // Main body with rounded vertical edges
            rounded_box_flat_top(enclosure_w, enclosure_h, enclosure_d, edge_r);
            
            // Belt tabs on short edges (extend along +/- X)
            for(x = [-1, 1]) {
                translate([x * (enclosure_w/2), 0, 0])
                    rotate([0, 0, x > 0 ? -90 : 90])
                        belt_tab(tab_total_w, tab_total_l, tab_thickness,
                                belt_slot_w, belt_slot_h, tab_r);
            }
            
            // Screw bosses in corners (full height)
            for(x = [-1, 1]) {
                for(y = [-1, 1]) {
                    translate([x * boss_x, y * boss_y, 0])
                        cylinder(d=boss_od, h=enclosure_d);
                }
            }
        }
        
        // Interior cavity for PCB (with boss zones excluded)
        difference() {
            translate([0, 0, wall + clearance + sensor_total_height/2])
                cube([sensor_pcb_w + 2*clearance, sensor_pcb_h + 2*clearance, 
                      sensor_total_height + 0.1], center=true);
            // Exclude boss zones from cavity cut
            for(x = [-1, 1]) {
                for(y = [-1, 1]) {
                    translate([x * boss_x, y * boss_y, -0.5])
                        cylinder(d=boss_od + 1, h=enclosure_d + 2);
                }
            }
        }
        
        // Lens opening (centered on top face)
        translate([0, 0, enclosure_d - wall/2])
            cube([lens_w + 1, lens_h + 1, wall + 1], center=true);
        
        // JST cable exit (SHORT EDGE at -X, centered on Y)
        translate([-(enclosure_w/2), 0, wall + clearance + jst_housing_h/2])
            cube([wall * 2 + 1, jst_housing_w + 2, jst_housing_h + 2], center=true);
        
        // JST cable channel (thinner cable portion)
        translate([-(enclosure_w/2), 0, wall + clearance + jst_cable_h/2])
            cube([wall * 2 + 1, jst_cable_w + 1, jst_cable_h + 1], center=true);
        
        // Holes cut through everything (including reinforcement pads)
        
        // Mounting screw holes through bottom (ribbed for wood screws)
        for(x = [-1, 1]) {
            for(y = [-1, 1]) {
                translate([x * (sensor_pcb_w/2 - hole_offset), 
                          y * (sensor_pcb_h/2 - hole_offset), 
                          -0.5])
                    ribbed_boss_hole(boss_hole, boss_rib_id, boss_rib_width, 
                                    boss_rib_count, enclosure_d + 1);
            }
        }
        
        // Ribbed screw boss holes
        for(x = [-1, 1]) {
            for(y = [-1, 1]) {
                translate([x * boss_x, y * boss_y, -0.5])
                    ribbed_boss_hole(boss_hole, boss_rib_id, boss_rib_width, 
                                    boss_rib_count, enclosure_d + 2);
            }
        }
    }
}

module front_enclosure_top() {
    
    sensor_pcb_w = 25;
    sensor_pcb_h = 17;
    
    enclosure_w = sensor_pcb_w + 2 * (clearance + wall);
    enclosure_h = sensor_pcb_h + 2 * (clearance + wall);
    lid_thickness = 3;
    
    // Match boss positions from bottom
    boss_x = enclosure_w/2 - boss_od/2 + 0.5;
    boss_y = enclosure_h/2 - boss_od/2 + 0.5;
    
    difference() {
        // Lid with rounded top edges
        rounded_box(enclosure_w, enclosure_h, lid_thickness, edge_r);
        
        // Sensor window (5mm along length x 6mm across, full penetration)
        cube([5, 6, lid_thickness * 4], center=true);
        
        // Screw clearance holes with counterbore for screw heads
        for(x = [-1, 1]) {
            for(y = [-1, 1]) {
                // Shaft clearance hole (full penetration)
                translate([x * boss_x, y * boss_y, -1])
                    cylinder(d=boss_clearance, h=lid_thickness + 4);
                // Counterbore for screw head (from top)
                translate([x * boss_x, y * boss_y, lid_thickness - boss_counterbore_depth])
                    cylinder(d=boss_counterbore_d, h=boss_counterbore_depth + 2);
            }
        }
    }
}

// ============================================================================
// HIP ENCLOSURE - FEATHER 32U4 + BATTERY
// ============================================================================

module hip_enclosure_bottom() {
    
    // Feather parameters
    feather_w = 22.5;
    feather_h = 52;
    feather_t = 3;
    feather_component_height = 5;
    
    // USB connector
    usb_w = 8;
    usb_h = 5;
    
    // Mounting holes
    hole_offset = 3;
    
    // Battery parameters
    battery_w = 30.5;
    battery_h = 37;
    battery_t = 5.5;
    
    // Battery connector
    bat_conn_w = 6.5;
    bat_conn_t = 7;
    
    // JST I2C cable entry - half-size
    jst_housing_w = 9.5 / 2;
    jst_housing_h = 5 / 2;
    
    // Enclosure dimensions
    internal_w = max(feather_w, battery_w) + 2 * clearance;
    internal_h = feather_h + battery_h + 3 * clearance;
    internal_d = max(feather_t + feather_component_height, battery_t) + 2 * clearance;
    
    enclosure_w = internal_w + 2 * wall;
    enclosure_h = internal_h + 2 * wall;
    enclosure_d = internal_d + wall;
    
    // Belt tab dimensions
    belt_slot_w = belt_width + 2 * loop_clearance;
    belt_slot_h = belt_thickness + 2 * loop_clearance;
    tab_total_w = belt_slot_w + 2 * tab_wall;
    tab_total_l = belt_slot_h + 2 * tab_wall;
    
    // Feather Y position
    feather_y_center = -(enclosure_h/2 - wall - clearance - feather_h/2);
    feather_bottom_edge_y = feather_y_center - feather_h/2;
    bat_terminal_y = feather_bottom_edge_y + 11.5;
    
    // Screw boss positions - inside wall with 1.5mm margin
    boss_x = enclosure_w/2 - boss_od/2 + 0.5;
    boss_y = enclosure_h/2 - boss_od/2 + 0.5;
    
    difference() {
        union() {
            // Main body with rounded vertical edges
            rounded_box_flat_top(enclosure_w, enclosure_h, enclosure_d, edge_r);
            
            // Belt tabs on long edge ends (extend along +/- Y)
            for(y = [-1, 1]) {
                translate([0, y * (enclosure_h/2), 0])
                    rotate([0, 0, y > 0 ? 0 : 180])
                        belt_tab(tab_total_w, tab_total_l, tab_thickness,
                                belt_slot_w, belt_slot_h, tab_r);
            }
            
            // Screw bosses in corners (full height)
            for(x = [-1, 1]) {
                for(y = [-1, 1]) {
                    translate([x * boss_x, y * boss_y, 0])
                        cylinder(d=boss_od, h=enclosure_d);
                }
            }
        }
        
        // Interior cavity (with boss zones excluded)
        difference() {
            translate([0, 0, wall + internal_d/2])
                cube([internal_w, internal_h, internal_d + 1], center=true);
            for(x = [-1, 1]) {
                for(y = [-1, 1]) {
                    translate([x * boss_x, y * boss_y, -0.5])
                        cylinder(d=boss_od + 1, h=enclosure_d + 2);
                }
            }
        }
        
        // Feather placement area
        translate([0, feather_y_center, wall + clearance])
            cube([feather_w + 2*clearance, feather_h + 2*clearance, 0.5], center=true);
        
        // Battery placement area
        translate([0, feather_y_center + feather_h/2 + clearance + battery_h/2, 
                  wall + clearance])
            cube([battery_w + 2*clearance, battery_h + 2*clearance, 0.5], center=true);
        
        // USB connector cutout (BOTTOM LONG EDGE, centered)
        translate([0, -(enclosure_h/2), wall + clearance + usb_h/2])
            cube([usb_w + 2, wall * 2 + 1, usb_h + 2], center=true);
        
        // Battery connector cutout (LEFT SHORT EDGE, 11.5mm from Feather USB end)
        translate([-(enclosure_w/2), bat_terminal_y, wall + clearance + bat_conn_t/2])
            cube([wall * 2 + 1, bat_conn_w + 2, bat_conn_t + 2], center=true);
        
        // Battery cable exit (LEFT SHORT EDGE, near battery)
        battery_y_center = feather_y_center + feather_h/2 + clearance + battery_h/2;
        translate([-(enclosure_w/2), battery_y_center, wall + clearance + bat_conn_t/2])
            cube([wall * 2 + 1, bat_conn_w + 2, bat_conn_t + 2], center=true);
        
        // JST I2C cable entry (FAR LONG EDGE, centered, near top)
        translate([0, (enclosure_h/2), enclosure_d - wall/2 - jst_housing_h/2])
            cube([jst_housing_w + 2, wall * 2 + 1, jst_housing_h + 2], center=true);
        
        // Reset button drill guide dimple (near USB on bottom edge, exterior)
        translate([0, -(enclosure_h/2 + 0.1), wall + clearance + 10])
            rotate([90, 0, 0])
                cylinder(d=2, h=1);
        
        // Mounting screw holes for Feather (ribbed for wood screws)
        for(x = [-1, 1]) {
            for(y = [-1, 1]) {
                translate([x * (feather_w/2 - hole_offset), 
                          feather_y_center + y * (feather_h/2 - hole_offset), 
                          -0.5])
                    ribbed_boss_hole(boss_hole, boss_rib_id, boss_rib_width, 
                                    boss_rib_count, wall + 2);
            }
        }
        
        // Ribbed screw boss holes
        for(x = [-1, 1]) {
            for(y = [-1, 1]) {
                translate([x * boss_x, y * boss_y, -0.5])
                    ribbed_boss_hole(boss_hole, boss_rib_id, boss_rib_width, 
                                    boss_rib_count, enclosure_d + 2);
            }
        }
    }
}

module hip_enclosure_top() {
    
    internal_w = max(22.5, 30.5) + 2 * clearance;
    internal_h = 52 + 37 + 3 * clearance;
    
    enclosure_w = internal_w + 2 * wall;
    enclosure_h = internal_h + 2 * wall;
    lid_thickness = 3;
    
    // Match boss positions from bottom
    boss_x = enclosure_w/2 - boss_od/2 + 0.5;
    boss_y = enclosure_h/2 - boss_od/2 + 0.5;
    
    difference() {
        // Lid with rounded top edges
        rounded_box(enclosure_w, enclosure_h, lid_thickness, edge_r);
        
        // Screw clearance holes with counterbore for screw heads
        for(x = [-1, 1]) {
            for(y = [-1, 1]) {
                // Shaft clearance hole (full penetration)
                translate([x * boss_x, y * boss_y, -1])
                    cylinder(d=boss_clearance, h=lid_thickness + 4);
                // Counterbore for screw head (from top)
                translate([x * boss_x, y * boss_y, lid_thickness - boss_counterbore_depth])
                    cylinder(d=boss_counterbore_d, h=boss_counterbore_depth + 2);
            }
        }
    }
}

// ============================================================================
// RENDER SELECTION
// ============================================================================

front_enclosure_bottom();

translate([70, 0, 0])
    front_enclosure_top();

translate([0, 170, 0])
    hip_enclosure_bottom();

translate([70, 170, 0])
    hip_enclosure_top();