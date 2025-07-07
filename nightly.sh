#!/bin/bash

cargo build --release

cp ~/fi-utils/target/release/fi_node ~/bin/nightly/fi_node
