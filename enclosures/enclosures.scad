// Belt-mounted wearable enclosure system for VL53L0X sensor + Feather 32u4
// Two-part design: Front sensor enclosure + Hip controller enclosure

// ============================================================================
// GLOBAL PARAMETERS
// ============================================================================

$fn = 50; // Smoothness for circles

// Common parameters
wall = 3; // Wall thickness
clearance = 1.5; // Component clearance
snap_tolerance = 0.2; // Snap fit tolerance
belt_width = 43; // Belt width
belt_thickness = 7; // Belt thickness at head
loop_clearance = 1; // Extra clearance for belt in loops

// ============================================================================
// FRONT ENCLOSURE - VL53L0X SENSOR
// ============================================================================

module front_enclosure_bottom() {
    
    // VL53L0X parameters
    sensor_pcb_w = 25;
    sensor_pcb_h = 17;
    sensor_pcb_t = 3;
    sensor_component_height = 2; // Additional height for components
    sensor_total_height = sensor_pcb_t + sensor_component_height;
    
    // Sensor lens
    lens_w = 4;
    lens_h = 5;
    
    // Mounting holes
    hole_dia = 4;
    hole_offset = 3; // From edges
    
    // JST connector (exits right SIDE along belt direction)
    jst_housing_w = 8;
    jst_housing_h = 3.5;
    jst_cable_w = 4;
    jst_cable_h = 1.5;
    
    // Enclosure dimensions
    enclosure_w = sensor_pcb_w + 2 * (clearance + wall);
    enclosure_h = sensor_pcb_h + 2 * (clearance + wall);
    enclosure_d = sensor_total_height + clearance + wall;
    
    // Belt tab parameters - flat tabs on short edges
    belt_slot_w = belt_width + 2 * loop_clearance;
    belt_slot_h = belt_thickness + 2 * loop_clearance;
    tab_wall = 3;
    tab_length = belt_slot_h + 2 * tab_wall; // How far tab extends from enclosure
    tab_width = belt_slot_w + 2 * tab_wall;   // Width of tab along edge
    
    difference() {
        union() {
            // Main body
            translate([0, 0, enclosure_d/2])
                cube([enclosure_w, enclosure_h, enclosure_d], center=true);
            
            // Belt tabs on short edges (extend outward along X, coplanar with bottom)
            for(x = [-1, 1]) {
                translate([x * (enclosure_w/2 + tab_length/2), 0, wall/2])
                    cube([tab_length, tab_width, wall], center=true);
            }
        }
        
        // Belt slots through tabs
        for(x = [-1, 1]) {
            translate([x * (enclosure_w/2 + tab_length/2), 0, wall/2])
                cube([belt_slot_h, belt_slot_w, wall + 1], center=true);
        }
        
        // Interior cavity for PCB
        translate([0, 0, wall + clearance + sensor_total_height/2])
            cube([sensor_pcb_w + 2*clearance, sensor_pcb_h + 2*clearance, sensor_total_height + 0.1], center=true);
        
        // Lens opening (centered on front face)
        translate([0, 0, enclosure_d - 0.5])
            cube([lens_w + 1, lens_h + 1, wall + 1], center=true);
        
        // JST cable exit (SHORT EDGE, centered)
        translate([-(enclosure_w/2 - wall/2), 0, wall + clearance + jst_housing_h/2])
            cube([wall + 1, jst_housing_w + 2, jst_housing_h + 2], center=true);
        
        // JST cable channel (thinner cable portion)
        translate([-(enclosure_w/2 - wall/2), 0, wall + clearance + jst_cable_h/2])
            cube([wall + 1, jst_cable_w + 1, jst_cable_h + 1], center=true);
        
        // Mounting screw holes through bottom
        for(x = [-1, 1]) {
            for(y = [-1, 1]) {
                translate([x * (sensor_pcb_w/2 - hole_offset), 
                          y * (sensor_pcb_h/2 - hole_offset), 
                          -0.5])
                    cylinder(d=2.5, h=wall + 1);
            }
        }
        
        // Snap post holes from top - EXTENDED to go through entire lid depth
        for(x = [-1, 1]) {
            for(y = [-1, 1]) {
                translate([x * (enclosure_w/2 - wall - 1.5), 
                          y * (enclosure_h/2 - wall - 1.5), 
                          enclosure_d - 6])
                    cylinder(d=3.4, h=10);
            }
        }
    }
    
    // Snap posts for lid (positioned away from PCB mounting holes)
    for(x = [-1, 1]) {
        for(y = [-1, 1]) {
            translate([x * (enclosure_w/2 - wall - 1.5), 
                      y * (enclosure_h/2 - wall - 1.5), 
                      wall])
                cylinder(d=3, h=enclosure_d - wall + 4);
        }
    }
}

module front_enclosure_top() {
    
    // Match bottom dimensions
    sensor_pcb_w = 25;
    sensor_pcb_h = 17;
    sensor_pcb_t = 3;
    sensor_component_height = 2;
    sensor_total_height = sensor_pcb_t + sensor_component_height;
    
    internal_w = sensor_pcb_w + 2 * clearance;
    internal_h = sensor_pcb_h + 2 * clearance;
    internal_d = sensor_total_height + clearance;
    
    enclosure_w = sensor_pcb_w + 2 * (clearance + wall);
    enclosure_h = sensor_pcb_h + 2 * (clearance + wall);
    lid_thickness = 2;
    
    difference() {
        union() {
            // Top lid
            translate([0, 0, lid_thickness/2])
                cube([enclosure_w, enclosure_h, lid_thickness], center=true);
            
            // Lip that fits inside
            translate([0, 0, -2])
                difference() {
                    cube([internal_w - snap_tolerance, internal_h - snap_tolerance, 4], center=true);
                    translate([0, 0, 0.5])
                        cube([internal_w - 2*wall, internal_h - 2*wall, 5], center=true);
                }
        }
        
        // Sensor window (centered, 5mm along length x 6mm across, through lid + lip)
        translate([0, 0, 0])
            cube([5, 6, 20], center=true);
        
        // Snap holes for posts - EXTENDED to go fully through lid and lip
        for(x = [-1, 1]) {
            for(y = [-1, 1]) {
                translate([x * (enclosure_w/2 - wall - 1.5), 
                          y * (enclosure_h/2 - wall - 1.5), 
                          -5])
                    cylinder(d=3.4, h=10);
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
    
    // USB connector - on bottom long edge, centered
    usb_w = 8;
    usb_h = 5;
    
    // Mounting holes
    hole_dia = 4;
    hole_offset = 3;
    
    // Battery parameters
    battery_w = 30.5;
    battery_h = 37;
    battery_t = 5.5;
    
    // Battery connector - on left short edge, 11.5mm from Feather's USB end
    bat_conn_w = 6.5;
    bat_conn_h = 3.5;
    bat_conn_t = 7;
    
    // JST I2C cable entry - right short edge (opposite USB), half-size
    jst_housing_w = 9.5 / 2;
    jst_housing_h = 5 / 2;
    
    // Enclosure dimensions
    internal_w = max(feather_w, battery_w) + 2 * clearance;
    internal_h = feather_h + battery_h + 3 * clearance;
    internal_d = max(feather_t + feather_component_height, battery_t) + 2 * clearance;
    
    enclosure_w = internal_w + 2 * wall;
    enclosure_h = internal_h + 2 * wall;
    enclosure_d = internal_d + wall;
    
    // Belt tab parameters - flat tabs on long edge ends
    belt_slot_w = belt_width + 2 * loop_clearance;
    belt_slot_h = belt_thickness + 2 * loop_clearance;
    tab_wall = 3;
    tab_length = belt_slot_h + 2 * tab_wall;
    tab_width = belt_slot_w + 2 * tab_wall;
    
    // Calculate Y position for Feather (bottom of enclosure)
    feather_y_center = -(enclosure_h/2 - wall - clearance - feather_h/2);
    
    // Feather bottom edge (the short edge where USB sits)
    feather_bottom_edge_y = feather_y_center - feather_h/2;
    
    // Battery terminal Y: centered 11.5mm from Feather's USB-side end
    bat_terminal_y = feather_bottom_edge_y + 11.5;
    
    difference() {
        union() {
            // Main bottom shell
            translate([0, 0, enclosure_d/2])
                cube([enclosure_w, enclosure_h, enclosure_d], center=true);
            
            // Belt tabs on long edge ends (extend outward along Y, coplanar with bottom)
            for(y = [-1, 1]) {
                translate([0, y * (enclosure_h/2 + tab_length/2), wall/2])
                    cube([tab_width, tab_length, wall], center=true);
            }
        }
        
        // Belt slots through tabs
        for(y = [-1, 1]) {
            translate([0, y * (enclosure_h/2 + tab_length/2), wall/2])
                cube([belt_slot_w, belt_slot_h, wall + 1], center=true);
        }
        
        // Interior cavity
        translate([0, 0, wall + internal_d/2])
            cube([internal_w, internal_h, internal_d + 1], center=true);
        
        // Feather placement area (bottom section)
        translate([0, feather_y_center, wall + clearance])
            cube([feather_w + 2*clearance, feather_h + 2*clearance, 0.5], center=true);
        
        // Battery placement area (top section)
        translate([0, feather_y_center + feather_h/2 + clearance + battery_h/2, wall + clearance])
            cube([battery_w + 2*clearance, battery_h + 2*clearance, 0.5], center=true);
        
        // USB connector cutout (BOTTOM LONG EDGE, centered)
        translate([0, 
                  -(enclosure_h/2 - wall/2), 
                  wall + clearance + usb_h/2])
            cube([usb_w + 2, wall + 1, usb_h + 2], center=true);
        
        // Battery connector cutout (LEFT SHORT EDGE, 11.5mm from Feather's USB end)
        translate([-(enclosure_w/2 - wall/2), 
                  bat_terminal_y, 
                  wall + clearance + bat_conn_t/2])
            cube([wall + 1, bat_conn_w + 2, bat_conn_t + 2], center=true);
        
        // JST I2C cable entry (FAR LONG EDGE, centered, near top of enclosure)
        translate([0, 
                  (enclosure_h/2 - wall/2), 
                  enclosure_d - wall/2 - jst_housing_h/2])
            cube([jst_housing_w + 2, wall + 1, jst_housing_h + 2], center=true);
        
        // Reset button pinhole (near USB on bottom edge)
        translate([0, 
                  -(enclosure_h/2 - 1), 
                  wall + clearance + 10])
            rotate([90, 0, 0])
                cylinder(d=1.5, h=wall + 1);
        
        // Mounting screw access for Feather
        for(x = [-1, 1]) {
            for(y = [-1, 1]) {
                translate([x * (feather_w/2 - hole_offset), 
                          feather_y_center + y * (feather_h/2 - hole_offset), 
                          -0.5])
                    cylinder(d=2.5, h=wall + 1);
            }
        }
        
        // Snap post holes from top
        translate([-(enclosure_w/2 - wall - 1.5), 
                  -(enclosure_h/2 - wall - 1.5), 
                  enclosure_d - 6])
            cylinder(d=3.4, h=10);
        
        translate([-(enclosure_w/2 - wall - 1.5), 
                  (enclosure_h/2 - wall - 1.5), 
                  enclosure_d - 6])
            cylinder(d=3.4, h=10);
        
        translate([(enclosure_w/2 - wall - 1.5), 
                  (enclosure_h/2 - wall - 1.5), 
                  enclosure_d - 6])
            cylinder(d=3.4, h=10);
    }
    
    // Snap posts for lid (only 3 posts to avoid battery terminal conflict)
    translate([-(enclosure_w/2 - wall - 1.5), 
              -(enclosure_h/2 - wall - 1.5), 
              wall])
        cylinder(d=3, h=enclosure_d - wall + 4);
    
    translate([-(enclosure_w/2 - wall - 1.5), 
              (enclosure_h/2 - wall - 1.5), 
              wall])
        cylinder(d=3, h=enclosure_d - wall + 4);
    
    translate([(enclosure_w/2 - wall - 1.5), 
              (enclosure_h/2 - wall - 1.5), 
              wall])
        cylinder(d=3, h=enclosure_d - wall + 4);
}

module hip_enclosure_top() {
    
    // Match bottom dimensions
    internal_w = max(22.5, 30.5) + 2 * clearance;
    internal_h = 52 + 37 + 3 * clearance;
    internal_d = max(3 + 5, 5.5) + 2 * clearance;
    
    enclosure_w = internal_w + 2 * wall;
    enclosure_h = internal_h + 2 * wall;
    lid_thickness = 2;
    
    difference() {
        union() {
            // Top lid
            translate([0, 0, lid_thickness/2])
                cube([enclosure_w, enclosure_h, lid_thickness], center=true);
            
            // Lip that fits inside
            translate([0, 0, -2])
                difference() {
                    cube([internal_w - snap_tolerance, internal_h - snap_tolerance, 4], center=true);
                    translate([0, 0, 0.5])
                        cube([internal_w - 2*wall, internal_h - 2*wall, 5], center=true);
                }
        }
        
        // Snap holes for posts
        translate([-(enclosure_w/2 - wall - 1.5), 
                  -(enclosure_h/2 - wall - 1.5), 
                  -5])
            cylinder(d=3.4, h=10);
        
        translate([-(enclosure_w/2 - wall - 1.5), 
                  (enclosure_h/2 - wall - 1.5), 
                  -5])
            cylinder(d=3.4, h=10);
        
        translate([(enclosure_w/2 - wall - 1.5), 
                  (enclosure_h/2 - wall - 1.5), 
                  -5])
            cylinder(d=3.4, h=10);
    }
}

// ============================================================================
// RENDER SELECTION
// ============================================================================

front_enclosure_bottom();

translate([0, 50, 0])
    front_enclosure_top();

translate([0, 150, 0])
    hip_enclosure_bottom();

translate([0, 290, 0])
    hip_enclosure_top();