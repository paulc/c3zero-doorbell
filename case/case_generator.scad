// Box Parameters
box_length = 100;
box_width = 75;
box_height = 20;
wall_thickness = 2;
corner_radius = 5;
lid_clearance = 0.2;
lid_height = 2;

// Screw pillar parameters
screw_hole_diameter = 3;
pillar_diameter = 8;
pillar_height = box_height - wall_thickness;

// Mounting hole parameters
mounting_holes = true;
mounting_hole_offset = 0.15;
mounting_top_diameter = 5;
mounting_screw_diameter = 2.5;

// PCB mount parameters
pcb_pillars = true;
pcb_width = 40;     // Between mounting holes
pcb_length = 60;    // Between mounting holes
pcb_pillar_height = 5;
pcb_pillar_diameter = 5;
pcb_hole_diameter = 1.5;

// Cutouts
front_cutout = false;
front_cutout_size = [5,5];
front_cutout_offset = [0,0];
front_cutout_radius = 2.5;

back_cutout = false;
back_cutout_size = [5,5];
back_cutout_offset = [0,0];
back_cutout_radius = 2.5;

left_cutout = false;
left_cutout_size = [5,5];
left_cutout_offset = [0,0];
left_cutout_radius = 2.5;

right_cutout = false;
right_cutout_size = [5,5];
right_cutout_offset = [0,0];
right_cutout_radius = 2.5;

ventilation_holes = false;
num_ventilation_holes = 5;

// Main box with flat top and bottom and screw pillars
module rounded_box(length, width, height, radius, thickness) {
    difference() {
        union() {
            difference() {
                // Outer shape with rounded sides but flat top/bottom
                linear_extrude(height)
                offset(r = radius) offset(delta = -radius)
                square([length, width], center = false);
         
                // Inner cutout
                translate([thickness, thickness, thickness])
                linear_extrude(height - thickness + 0.1)
                offset(r = max(radius - thickness, 0)) 
                offset(delta = -(max(radius - thickness, 0)))
                square([length - thickness*2, width - thickness*2], center = false);
                
                
            }
            // Add four screw pillars
            for (pos = [
                [radius, radius, 0],
                [length - radius, radius, 0],
                [radius, width - radius, 0],
                [length - radius, width - radius, 0]
            ]) {
                translate(pos)
                cylinder(h = pillar_height, d = pillar_diameter, $fn = 20);
            }
            // PCB Pillars
            if (pcb_pillars) {
                pcb_pillars();  
            }
        }
        // Screw holes through pillars
        for (pos = [
            [radius, radius, wall_thickness],
            [length - radius, radius, wall_thickness],
            [radius, width - radius, wall_thickness],
            [length - radius, width - radius, wall_thickness]
        ]) {
            translate(pos)
            cylinder(h = pillar_height + 0.1, d = screw_hole_diameter, $fn = 20);
        }

        // Mounting holes
        if (mounting_holes) {
            mounting_hole(box_length * mounting_hole_offset, box_width/2);
            mounting_hole(box_length * (1 - mounting_hole_offset), box_width/2);
        }
        
        // Cutout
        if (front_cutout) {
                side_cutout("front", 
                    size = front_cutout_size, 
                    offset = front_cutout_offset,
                    radius = front_cutout_radius);
        } 
        if (back_cutout) {
                side_cutout("back", 
                    size = back_cutout_size, 
                    offset = back_cutout_offset,
                    radius = back_cutout_radius);
        } 
        if (left_cutout) {
                side_cutout("left", 
                    size = left_cutout_size, 
                    offset = left_cutout_offset,
                    radius = left_cutout_radius);
        } 
        if (right_cutout) {
                side_cutout("right", 
                    size = right_cutout_size, 
                    offset = right_cutout_offset,
                    radius = right_cutout_radius);
        } 
    }
}

module side_cutout(
    side,               // "front", "back", "left", "right"
    size = [5,5],       // width, height
    offset = [0,0],     // left/right, up/down
    radius = 0
) {
    width = size[0];
    height = size[1];
    
    // Make sure radius is less than width/height
    radius = (radius < min(width/2,height/2)) ? radius : min(width/2,height/2) - 0.01;
    
    lr = offset[0];
    ud = offset[1];
    
    if (side == "front") {
        translate([0 - 0.01, box_width/2 - width / 2 + lr, box_height/2 - height/2 + ud])
            rotate([90,0,90])
            linear_extrude(height = wall_thickness + 0.02, $fn = 20)
                offset (r = radius) offset(delta = -radius)
                square([width,height]);
    } else if (side == "back") {
         translate([box_length - wall_thickness - 0.01, box_width/2 - width/2 + lr, box_height/2 - height/2 + ud])
            rotate([90,0,90])
            linear_extrude(height = wall_thickness + 0.02, $fn = 20)
                offset (r = radius) offset(delta = -radius)
                square([width,height]);   
    } else if (side == "left") {
         translate([box_length/2 - width/2 + lr, wall_thickness + 0.01, box_height/2 - height/2 + ud])
            rotate([90,0,0])
            linear_extrude(height = wall_thickness + 0.02, $fn = 20)
                offset (r = radius) offset(delta = -radius)
                square([width,height]);   
    } else if (side == "right") {
         translate([box_length/2 - width/2 + lr, box_width + 0.01, box_height/2 - height/2 + ud])
            rotate([90,0,0])
            linear_extrude(height = wall_thickness + 0.02, $fn = 20)
                offset (r = radius) offset(delta = -radius)
                square([width,height]);   
    }
}


module pcb_pillars() {
    y0 = (box_width - pcb_width) / 2;
    y1 = y0 + pcb_width;
    x0 = (box_length - pcb_length) / 2;
    x1 = x0 + pcb_length;
    
    for (pos = [[x0,y0,0],[x0,y1,0],[x1,y0,0],[x1,y1,0]]) {
        translate(pos) 
        difference() {
            cylinder(h = pcb_pillar_height + wall_thickness, d = pcb_pillar_diameter, $fn = 20);
            translate([0,0,wall_thickness])
                cylinder(h = pcb_pillar_height + 0.01, d = pcb_hole_diameter, $fn = 20);
        }
    }
}

module mounting_hole(x,y) {
        // We add a very small +/- z delta to make preview render clearer
        translate([x,y,-0.01])
            linear_extrude(wall_thickness + 0.02)
                rotate([0,0,180])
                union() {
                    circle(d = mounting_top_diameter, $fn=10);
                    translate([mounting_top_diameter/2,0,0])
                        offset(r = mounting_screw_diameter/2, $fn = 10) 
                            square([mounting_screw_diameter,0.1], center= true);
                }
}

// Lid with flat bottom and screw holes
module rounded_lid(length, width, height, radius, thickness) {
    difference() {
        union() {
            // Main lid plate with flat bottom
            linear_extrude(height)
            offset(r = radius) offset(delta = -radius)
            square([length, width], center = false);
            
            // Lip that fits inside the box
            translate([thickness, thickness, height])
            linear_extrude(height)
            offset(r = max(radius - thickness - lid_clearance, 0))
            offset(delta = -(max(radius - thickness - lid_clearance, 0)))
            square([
                length - thickness*2 - lid_clearance*2, 
                width - thickness*2 - lid_clearance*2
            ], center = false);
        }
        
        // Screw holes in lid
        for (pos = [
            [radius, radius, 0],
            [length - radius, radius, 0],
            [radius, width - radius, 0],
            [length - radius, width - radius, 0]
        ]) {
            translate(pos)
            cylinder(h = height*2, d = screw_hole_diameter + 0.5, $fn = 10); // Slightly larger for clearance
        }
        
        // Ventilation holes
        if (ventilation_holes) {
            for (i = [0:num_ventilation_holes-1] ) {
                translate([length * (0.1 + i/20), width * 0.1, -0.01])
                    linear_extrude(height * 2 + 0.02)
                        offset(r = thickness/2, $fn = 10) offset(delta = -thickness/2)
                        square([height + 0.01, width * 0.375]);
                translate([length * (0.1 + i/20), width * 0.525, -0.01])
                    linear_extrude(height * 2 + 0.02)
                        offset(r = thickness/2, $fn = 10) offset(delta = -thickness/2)
                        square([height + 0.01, width * 0.375]);
            }    
        }
    }
}

// Create the box
rounded_box(box_length, box_width, box_height, corner_radius, wall_thickness);

// Create the lid (positioned next to the box for printing)
translate([box_length + 20, 0, 0])
    rounded_lid(box_length, box_width, lid_height, corner_radius, wall_thickness);
