cd ..
git restore easy-fs-fuse/src/main.rs os/Makefile
cd ci-user
make test CHAPTER=8
cd ..
git restore easy-fs-fuse/src/main.rs os/Makefile
cd os
