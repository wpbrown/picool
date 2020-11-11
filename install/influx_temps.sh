#!/bin/bash
get_temp_f() {
    tempc=$(cat $1)
    bc <<< "scale=3;((9/5) * ($tempc/1000)) + 32"
}

delta_c_to_f() {
    bc <<< "scale=3;((9/5) * ($1))"
}

tempf_ref=$(get_temp_f $REFRIGERATOR_SENSOR_PATH)
tempf_fre=$(get_temp_f $FREEZER_SENSOR_PATH)
tempf_amb=$(get_temp_f $AMBIENT_SENSOR_PATH)

echo "temperature ambient=$tempf_amb,freezer=$tempf_fre,refrigerator=$tempf_ref"

ref_sensor_name=$(basename $(dirname $REFRIGERATOR_SENSOR_PATH))
ref_comp_path="/var/lib/picool/comp_${ref_sensor_name}"

if [[ -f $ref_comp_path ]]; then
    read -r deltc_low deltc_hig < $ref_comp_path
    if [[ -n $deltc_low && -n $deltc_hig ]]; then
        deltf_low=$(delta_c_to_f $deltc_low)
        deltf_hig=$(delta_c_to_f $deltc_hig)
        echo "compensation low=$deltf_low,high=$deltf_hig"
    fi
fi
