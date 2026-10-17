#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p .build/ddc-tests
clang -fmodules -Wall -Wextra -Werror -I ThirdParty/m1ddc/headers \
  Tests/DDC/ReplyTests.m ThirdParty/m1ddc/sources/i2c.m \
  -framework Foundation -o .build/ddc-tests/reply-tests
.build/ddc-tests/reply-tests
clang -fmodules -Wall -Wextra -Werror -I ThirdParty/m1ddc/headers \
  Tests/DDC/RouteTests.m -framework Foundation -framework CoreDisplay \
  -o .build/ddc-tests/route-tests
.build/ddc-tests/route-tests
