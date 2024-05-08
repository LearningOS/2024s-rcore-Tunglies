cd ../ci-user
make test CHAPTER=6
cd -
git restore Makefile build.rs ../easy-fs-fuse/src/main.rs