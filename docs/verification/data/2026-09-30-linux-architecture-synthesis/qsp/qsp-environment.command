devenv shell -- /usr/bin/bash -c rustc\ -vV\;\ cargo\ -V\;\ env\ \|\ sort\ \|\ grep\ -E\ \"\^\(OMP_\|QUEST_\|CARGO_BUILD_JOBS\|CC=\|CXX=\|CMAKE_PREFIX_PATH=\)\" 
