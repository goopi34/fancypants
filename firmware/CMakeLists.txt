cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(fancypants_nrf52)

target_sources(app PRIVATE
  src/main.c
  src/range_service.c
  src/battery.c
)
