cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(fancypants_nrf52)

if(DEFINED APP_VERSION_STRING)
  add_compile_definitions(APP_VERSION_STRING="${APP_VERSION_STRING}")
endif()

target_sources(app PRIVATE
  src/main.c
  src/range_service.c
  src/battery.c
)
