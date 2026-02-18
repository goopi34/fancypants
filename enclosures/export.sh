#!/bin/bash
# Export each OpenSCAD object as a separate STL file
# Usage: ./export_scad.sh [-v] <source.scad> [output_dir]
VERBOSE=0
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

# Create a clean library version by stripping the top-level render calls.
# This removes everything from the RENDER SELECTION comment to EOF.
CLEAN_SOURCE="/tmp/scad_export_library.scad"
sed '/^\/\/ RENDER SELECTION/,$ d' "$SOURCE" > "$CLEAN_SOURCE"

export_object() {
    local object_name=$1
    local temp_file="/tmp/scad_export_${object_name}.scad"

    {
        echo "include <$CLEAN_SOURCE>"
        echo "${object_name}();"
    } > "$temp_file"

    [ $VERBOSE -eq 1 ] && echo "Debug: Rendering $object_name" && cat "$temp_file"

    openscad -o "$OUTPUT_DIR/$object_name.stl" "$temp_file" 2>&1
    echo "Exported: $OUTPUT_DIR/$object_name.stl"
    rm "$temp_file"
}

export_object "front_enclosure_bottom"
export_object "front_enclosure_top"
export_object "hip_enclosure_bottom"
export_object "hip_enclosure_top"

rm "$CLEAN_SOURCE"
echo "Done."