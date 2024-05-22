cd ..
git restore easy-fs-fuse/src/main.rs os/Makefile
cd -

echo "ch8_deadlock_sem1" | make run BASE=8 LOG=$FLAG
