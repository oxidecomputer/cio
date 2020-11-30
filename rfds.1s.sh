#!/bin/bash

# <bitbar.title>RFDs</bitbar.title>
# <bitbar.version>v0.1</bitbar.version>
# <bitbar.author>Jess Frazelle</bitbar.author>
# <bitbar.author.github>jessfraz</bitbar.author.github>
# <bitbar.desc>Show a list of Oxide Computer Company RFDs</bitbar.desc>
# <bitbar.image></bitbar.image> <!-- fix me -->

jq=/usr/local/bin/jq
output=$(curl -s api.internal.oxide.computer/rfds)
RFD_COUNT=$(echo "$output" | $jq length)
RFDC_FORMATTED=`printf "%'.f\n" $RFD_COUNT`
echo "$RFDC_FORMATTED RFDs"
echo ---
for row in $(echo "${output}" | $jq -r 'reverse | .[] | @base64'); do
    _jq() {
		echo ${row} | base64 --decode | $jq -r ${1}
    }

	echo "$(_jq '.name') | size=14 font=Courier href=$(_jq '.link')"
done
