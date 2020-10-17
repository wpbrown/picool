#!/bin/bash
get_temp_f() {
    tempc=$(cat $1)
    bc <<< "scale=3;((9/5) * ($tempc/1000)) + 32"
}

tempf_ref=$(get_temp_f $REFRIGERATOR_SENSOR_PATH)
tempf_fre=$(get_temp_f $FREEZER_SENSOR_PATH)
tempf_amb=$(get_temp_f $AMBIENT_SENSOR_PATH)

echo "temperature ambient=$tempf_amb,freezer=$tempf_fre,refrigerator=$tempf_ref"
