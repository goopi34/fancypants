#!/bin/bash

# Export each OpenSCAD object as a separate STL file
# Usage: ./export_scad.sh [-v] <source.scad> [output_dir]

VERBOSE=0

# Parse flags
while getopts "v" opt; do
    case $opt in
        v) VERBOSE=1 ;;
        *) echo "Usage: $0 [-v] <source.scad> [output_dir]"; exit 1 ;;
    esac
done
shift $((OPTIND - 1))

SOURCE="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
OUTPUT_DIR="${2:-.}"

if [ ! -f "$SOURCE" ]; then
    echo "Error: Source file '$SOURCE' not found"
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

# Create a temporary modified version for each export
TEMP_FILE="/tmp/scad_export_temp.scad"

export_object() {
    local object_num=$1
    local object_name=$2
    local temp_file="/tmp/scad_export_$object_num.scad"
    
    # Create include + object render
    {
        echo "include <$SOURCE>"
        case $object_num in
            1) echo "front_enclosure_bottom();" ;;
            2) echo "translate([70, 0, 0]) front_enclosure_top();" ;;
            3) echo "translate([0, 170, 0]) hip_enclosure_bottom();" ;;
            4) echo "translate([70, 170, 0]) hip_enclosure_top();" ;;
        esac
    } > "$temp_file"
    
    [ $VERBOSE -eq 1 ] && echo "Debug: Rendering $object_name from $temp_file"
    [ $VERBOSE -eq 1 ] && cat "$temp_file"
    
    # Export to STL
    openscad -o "$OUTPUT_DIR/$object_name.stl" "$temp_file" 2>&1
    echo "Exported: $OUTPUT_DIR/$object_name.stl"
    rm "$temp_file"
}

export_object 1 "front_enclosure_bottom"
export_object 2 "front_enclosure_top"
export_object 3 "hip_enclosure_bottom"
export_object 4 "hip_enclosure_top"

echo "Done."