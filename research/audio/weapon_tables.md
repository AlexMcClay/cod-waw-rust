#### `zombie_colt`

Player sound fields: firePlr=`weap_colt_fire_plr`, fireLastPlr=`weap_colt_fire_plr`, emptyFirePlr=`dryfire_pistol_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_small_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_colt45_reload_notempty` | 62 f @ 24 fps = 2.58s | 1900 ms | f9 / t0.145 / **0.28s** `gr_1911_mag_out_plr`<br>f39 / t0.629 / **1.20s** `gr_1911_futz_plr`<br>f43 / t0.694 / **1.32s** `gr_1911_mag_in_plr` |
| reloadEmpty | `viewmodel_colt45_reload` | 70 f @ 24 fps = 2.92s | 2650 ms | f9 / t0.129 / **0.34s** `gr_1911_mag_out_plr`<br>f35 / t0.500 / **1.32s** `gr_1911_futz_plr`<br>f42 / t0.600 / **1.59s** `gr_1911_mag_in_plr`<br>f56 / t0.800 / **2.12s** `gr_1911_slide_forward_plr` |

#### `sw_357`

Player sound fields: firePlr=`weap_357_fire_plr`, fireLastPlr=`weap_357_fire_plr`, emptyFirePlr=`dryfire_pistol_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_small_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_sw357_reload` | 89 f @ 30 fps = 2.97s | 3000 ms | f17 / t0.191 / **0.57s** `gr_357_open`<br>f40 / t0.449 / **1.35s** `gr_357_empty`<br>f62 / t0.697 / **2.09s** `gr_357_load`<br>f79 / t0.888 / **2.66s** `gr_357_close` |
| reloadEmpty | `viewmodel_sw357_reload` | 89 f @ 30 fps = 2.97s | 3000 ms | f17 / t0.191 / **0.57s** `gr_357_open`<br>f40 / t0.449 / **1.35s** `gr_357_empty`<br>f62 / t0.697 / **2.09s** `gr_357_load`<br>f79 / t0.888 / **2.66s** `gr_357_close` |

#### `kar98k`

Player sound fields: firePlr=`weap_kar98k_fire_plr`, fireLastPlr=`weap_kar98k_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| rechamber | `viewmodel_kar98_rechamber` | 31 f @ 30 fps = 1.03s | 1000 ms | f7 / t0.226 / **0.23s** `gr_kar98_bolt_up_plr`<br>f11 / t0.355 / **0.35s** `gr_kar98_bolt_back_plr`<br>f19 / t0.613 / **0.61s** `gr_kar98_bolt_front_plr` |
| reload | `viewmodel_kar98_reload` | 79 f @ 30 fps = 2.63s | 2500 ms | f5 / t0.063 / **0.16s** `gr_kar98_bolt_up_plr`<br>f9 / t0.114 / **0.28s** `gr_kar98_bolt_back_plr`<br>f44 / t0.557 / **1.39s** `gr_kar98_clip_in_plr`<br>f64 / t0.810 / **2.03s** `gr_kar98_bolt_front_plr`<br>f65 / t0.823 / **2.06s** `gr_kar98_clip_eject_plr`<br>f71 / t0.899 / **2.25s** `gr_kar98_bolt_down_plr` |
| reloadEmpty | `viewmodel_kar98_reload` | 79 f @ 30 fps = 2.63s | 2500 ms | f5 / t0.063 / **0.16s** `gr_kar98_bolt_up_plr`<br>f9 / t0.114 / **0.28s** `gr_kar98_bolt_back_plr`<br>f44 / t0.557 / **1.39s** `gr_kar98_clip_in_plr`<br>f64 / t0.810 / **2.03s** `gr_kar98_bolt_front_plr`<br>f65 / t0.823 / **2.06s** `gr_kar98_clip_eject_plr`<br>f71 / t0.899 / **2.25s** `gr_kar98_bolt_down_plr` |
| adsRechamber | `viewmodel_kar98_rechamber` | 31 f @ 30 fps = 1.03s | 1000 ms | f7 / t0.226 / **0.23s** `gr_kar98_bolt_up_plr`<br>f11 / t0.355 / **0.35s** `gr_kar98_bolt_back_plr`<br>f19 / t0.613 / **0.61s** `gr_kar98_bolt_front_plr` |

#### `springfield`

Player sound fields: firePlr=`weap_springfield_fire_plr`, fireLastPlr=`weap_springfield_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| rechamber | `viewmodel_springfield_rechamber` | 36 f @ 30 fps = 1.20s | 1000 ms | f5 / t0.139 / **0.14s** `gr_springfield_bolt_up_plr`<br>f11 / t0.306 / **0.31s** `gr_springfield_bolt_back_plr`<br>f19 / t0.528 / **0.53s** `gr_springfield_bolt_front_plr`<br>f23 / t0.639 / **0.64s** `gr_springfield_bolt_down_plr` |
| reload | `viewmodel_springfield_reload` | 92 f @ 30 fps = 3.07s | 3200 ms | f12 / t0.130 / **0.42s** `gr_springfield_bolt_up_plr`<br>f18 / t0.196 / **0.63s** `gr_springfield_bolt_back_plr`<br>f55 / t0.598 / **1.91s** `gr_springfield_clip_in_plr`<br>f78 / t0.848 / **2.71s** `gr_springfield_clip_eject_plr`<br>f78 / t0.848 / **2.71s** `gr_springfield_bolt_front_plr`<br>f83 / t0.902 / **2.89s** `gr_springfield_bolt_down_plr`<br>f85 / t0.924 / **2.96s** `gr_springfield_clip_land_plr` **(not in map)** |
| reloadEmpty | `viewmodel_springfield_reload` | 92 f @ 30 fps = 3.07s | 3200 ms | f12 / t0.130 / **0.42s** `gr_springfield_bolt_up_plr`<br>f18 / t0.196 / **0.63s** `gr_springfield_bolt_back_plr`<br>f55 / t0.598 / **1.91s** `gr_springfield_clip_in_plr`<br>f78 / t0.848 / **2.71s** `gr_springfield_clip_eject_plr`<br>f78 / t0.848 / **2.71s** `gr_springfield_bolt_front_plr`<br>f83 / t0.902 / **2.89s** `gr_springfield_bolt_down_plr`<br>f85 / t0.924 / **2.96s** `gr_springfield_clip_land_plr` **(not in map)** |
| adsRechamber | `viewmodel_springfield_rechamber` | 36 f @ 30 fps = 1.20s | 1000 ms | f5 / t0.139 / **0.14s** `gr_springfield_bolt_up_plr`<br>f11 / t0.306 / **0.31s** `gr_springfield_bolt_back_plr`<br>f19 / t0.528 / **0.53s** `gr_springfield_bolt_front_plr`<br>f23 / t0.639 / **0.64s** `gr_springfield_bolt_down_plr` |

#### `m1carbine`

Player sound fields: firePlr=`weap_carbine_fire_plr`, fireLastPlr=`weap_carbine_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`carbine_first_raise`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`carbine_first_raise`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_carbine_reload` | 95 f @ 30 fps = 3.17s | 2900 ms | f23 / t0.242 / **0.70s** `gr_m1carbine_mag_out_plr`<br>f50 / t0.526 / **1.53s** `gr_m1carbine_futz_plr`<br>f55 / t0.579 / **1.68s** `gr_m1carbine_mag_in_plr`<br>f71 / t0.747 / **2.17s** `gr_m1carbine_tap_plr` |
| reloadEmpty | `viewmodel_carbine_reload_empty` | 111 f @ 30 fps = 3.70s | 3700 ms | f20 / t0.180 / **0.67s** `gr_m1carbine_mag_out_plr`<br>f50 / t0.451 / **1.67s** `gr_m1carbine_futz_plr`<br>f54 / t0.486 / **1.80s** `gr_m1carbine_mag_in_plr`<br>f71 / t0.640 / **2.37s** `gr_m1carbine_tap_plr`<br>f94 / t0.847 / **3.13s** `gr_m1carbine_charge_plr` |

#### `gewehr43`

Player sound fields: firePlr=`weap_g43_fire_plr`, fireLastPlr=`weap_g43_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`gewehr_first_raise`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`gewehr_first_raise`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_g43_noscope_partial_reload` | 99 f @ 30 fps = 3.30s | 3000 ms | f18 / t0.182 / **0.55s** `gr_g43_mag_out_plr`<br>f51 / t0.515 / **1.55s** `gr_g43_mag_in_plr`<br>f66 / t0.667 / **2.00s** `gr_g43_tap_plr` |
| reloadEmpty | `viewmodel_g43_noscope_reload` | 142 f @ 30 fps = 4.73s | 4000 ms | f26 / t0.183 / **0.73s** `gr_g43_tap_plr`<br>f33 / t0.232 / **0.93s** `gr_g43_mag_out_plr`<br>f63 / t0.444 / **1.77s** `gr_g43_mag_in_plr`<br>f82 / t0.578 / **2.31s** `gr_g43_tap_plr`<br>f118 / t0.831 / **3.32s** `gr_kar98_bolt_front_plr` |

#### `m1garand`

Player sound fields: firePlr=`weap_m1garand_fire_plr`, fireLastPlr=`weap_m1garand_lastshot_plr`, emptyFirePlr=`dryfire_rifle_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_m1garand_partial_reload` | 105 f @ 24 fps = 4.38s | 3400 ms | f12 / t0.114 / **0.39s** `gr_m1gar_pull_slide_plr`<br>f22 / t0.209 / **0.71s** `gr_m1gar_clip_out_plr`<br>f25 / t0.238 / **0.81s** `gr_m1gar_futz_plr` **(no alias)**<br>f29 / t0.276 / **0.94s** `gr_m1gar_breech_close_plr`<br>f65 / t0.619 / **2.10s** `gr_m1gar_pull_slide_plr`<br>f75 / t0.714 / **2.43s** `gr_m1gar_clip_in_plr`<br>f82 / t0.781 / **2.66s** `gr_m1gar_breech_close_plr` |
| reloadEmpty | `viewmodel_m1garand_reload` | 46 f @ 24 fps = 1.92s | 1600 ms | f17 / t0.370 / **0.59s** `gr_m1garand_futz_plr` **(no alias)**<br>f24 / t0.522 / **0.83s** `gr_m1gar_clip_in_plr`<br>f33 / t0.717 / **1.15s** `gr_m1gar_breech_close_plr` |
| firstRaise | `viewmodel_m1garand_first_pullout` | 64 f @ 24 fps = 2.67s | 1000 ms | f16 / t0.250 / **0.25s** `gr_m1gar_pull_slide_plr`<br>f32 / t0.500 / **0.50s** `gr_m1gar_breech_close_plr` |

#### `m1garand_gl`

Player sound fields: firePlr=`weap_m1garand_fire_plr`, fireLastPlr=`weap_m1garand_lastshot_plr`, emptyFirePlr=`dryfire_rifle_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_m1garand_partial_reload` | 105 f @ 24 fps = 4.38s | 3400 ms | f12 / t0.114 / **0.39s** `gr_m1gar_pull_slide_plr`<br>f22 / t0.209 / **0.71s** `gr_m1gar_clip_out_plr`<br>f25 / t0.238 / **0.81s** `gr_m1gar_futz_plr` **(no alias)**<br>f29 / t0.276 / **0.94s** `gr_m1gar_breech_close_plr`<br>f65 / t0.619 / **2.10s** `gr_m1gar_pull_slide_plr`<br>f75 / t0.714 / **2.43s** `gr_m1gar_clip_in_plr`<br>f82 / t0.781 / **2.66s** `gr_m1gar_breech_close_plr` |
| reloadEmpty | `viewmodel_m1garand_reload` | 46 f @ 24 fps = 1.92s | 1600 ms | f17 / t0.370 / **0.59s** `gr_m1garand_futz_plr` **(no alias)**<br>f24 / t0.522 / **0.83s** `gr_m1gar_clip_in_plr`<br>f33 / t0.717 / **1.15s** `gr_m1gar_breech_close_plr` |
| firstRaise | `viewmodel_m1garand_first_pullout` | 64 f @ 24 fps = 2.67s | 1000 ms | f16 / t0.250 / **0.25s** `gr_m1gar_pull_slide_plr`<br>f32 / t0.500 / **0.50s** `gr_m1gar_breech_close_plr` |

#### `thompson`

Player sound fields: firePlr=`weap_thompson_fire_plr`, emptyFirePlr=`dryfire_smg_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_thompson_reload` | 58 f @ 30 fps = 1.93s | 2100 ms | f13 / t0.224 / **0.47s** `gr_thompson_mag_out_plr`<br>f41 / t0.707 / **1.48s** `gr_thompson_mag_in_plr` |
| reloadEmpty | `viewmodel_thompson_reload_empty` | 77 f @ 30 fps = 2.57s | 2450 ms | f13 / t0.169 / **0.41s** `gr_thompson_mag_out_plr`<br>f30 / t0.390 / **0.95s** `gr_thompson_mag_in_plr`<br>f52 / t0.675 / **1.65s** `gr_thompson_charge_plr` |

#### `mp40`

Player sound fields: firePlr=`weap_mp40_fire_plr`, fireLastPlr=`weap_mp40_fire_plr`, emptyFirePlr=`dryfire_smg_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_mp40_reload` | 78 f @ 30 fps = 2.60s | 2300 ms | f18 / t0.231 / **0.53s** `gr_mp40_mag_out_plr`<br>f49 / t0.628 / **1.44s** `gr_mp40_futz_plr`<br>f61 / t0.782 / **1.80s** `gr_mp40_mag_in_plr` |
| reloadEmpty | `viewmodel_mp40_reload_empty` | 100 f @ 30 fps = 3.33s | 2900 ms | f18 / t0.180 / **0.52s** `gr_mp40_mag_out_plr`<br>f51 / t0.510 / **1.48s** `gr_mp40_futz_plr`<br>f61 / t0.610 / **1.77s** `gr_mp40_mag_in_plr`<br>f81 / t0.810 / **2.35s** `gr_mp40_charge_plr` |
| firstRaise | `viewmodel_mp40_first_raise` | 50 f @ 30 fps = 1.67s | 1330 ms | f19 / t0.380 / **0.51s** `gr_mp40_charge_plr` |

#### `stg44`

Player sound fields: firePlr=`weap_MP44_fire_plr`, fireLastPlr=`weap_MP44_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`stg44_first_raise`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`stg44_first_raise`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_mp44_reload` | 60 f @ 30 fps = 2.00s | 2150 ms | f11 / t0.183 / **0.39s** `gr_mp44_mag_out_plr`<br>f34 / t0.567 / **1.22s** `gr_mp44_futz_plr`<br>f41 / t0.683 / **1.47s** `gr_mp44_mag_in_plr` |
| reloadEmpty | `viewmodel_mp44_reload_empty` | 85 f @ 30 fps = 2.83s | 2800 ms | f11 / t0.129 / **0.36s** `gr_mp44_mag_out_plr`<br>f34 / t0.400 / **1.12s** `gr_mp44_futz_plr`<br>f37 / t0.435 / **1.22s** `gr_mp44_mag_in_plr`<br>f63 / t0.741 / **2.08s** `gr_mp44_charge_plr` |

#### `bar`

Player sound fields: firePlr=`weap_bar_fire_plr`, fireLastPlr=`weap_bar_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_bar_reload` | 106 f @ 30 fps = 3.53s | 2750 ms | f27 / t0.255 / **0.70s** `gr_bar_mag_out_plr`<br>f59 / t0.557 / **1.53s** `gr_bar_mag_in_plr`<br>f73 / t0.689 / **1.89s** `gr_bar_tap_plr` |
| reloadEmpty | `viewmodel_bar_reload_empty` | 137 f @ 30 fps = 4.57s | 3250 ms | f27 / t0.197 / **0.64s** `gr_bar_mag_out_plr`<br>f58 / t0.423 / **1.38s** `gr_bar_mag_in_plr`<br>f73 / t0.533 / **1.73s** `gr_bar_tap_plr`<br>f102 / t0.745 / **2.42s** `gr_bar_charge_plr` |

#### `fg42_bipod`

Player sound fields: firePlr=`weap_fg42_fire_plr`, fireLastPlr=`weap_fg42_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_fg42_partial_reload` | 89 f @ 30 fps = 2.97s | 2400 ms | f15 / t0.169 / **0.40s** `gr_fg42_mag_out_plr`<br>f61 / t0.685 / **1.64s** `gr_fg42_futz_plr`<br>f67 / t0.753 / **1.81s** `gr_fg42_mag_in_plr` |
| reloadEmpty | `viewmodel_fg42_reload` | 148 f @ 30 fps = 4.93s | 3500 ms | f15 / t0.101 / **0.35s** `gr_fg42_mag_out_plr`<br>f61 / t0.412 / **1.44s** `gr_fg42_futz_plr`<br>f67 / t0.453 / **1.58s** `gr_fg42_mag_in_plr`<br>f110 / t0.743 / **2.60s** `gr_fg42_pull_plr`<br>f116 / t0.784 / **2.74s** `gr_fg42_release_plr` |
| deploy | `viewmodel_fg42_deploy` | 40 f @ 30 fps = 1.33s | 1400 ms | f11 / t0.275 / **0.39s** `gr_mg_deploy_start`<br>f17 / t0.425 / **0.59s** `gr_mg_deploy_end` |
| breakdown | `viewmodel_fg42_breakdown` | 40 f @ 30 fps = 1.33s | 1300 ms | f10 / t0.250 / **0.33s** `gr_mg_break_down` |

#### `mg42_bipod`

Player sound fields: firePlr=`weap_mg42_fire_plr`, fireLastPlr=`weap_mg42_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_mg42_reload` | 150 f @ 30 fps = 5.00s | 4500 ms | f18 / t0.120 / **0.54s** `gr_mg42_jimmy_plr`<br>f25 / t0.167 / **0.75s** `gr_mg42_mag_out_plr`<br>f55 / t0.367 / **1.65s** `gr_mg42_futz_plr`<br>f64 / t0.427 / **1.92s** `gr_mg42_mag_in_plr`<br>f101 / t0.673 / **3.03s** `gr_mg42_pull_plr`<br>f111 / t0.740 / **3.33s** `gr_mg42_charge_plr` |
| reloadEmpty | `viewmodel_mg42_reload_empty` | 150 f @ 30 fps = 5.00s | 4500 ms | f18 / t0.120 / **0.54s** `gr_mg42_jimmy_plr`<br>f25 / t0.167 / **0.75s** `gr_mg42_mag_out_plr`<br>f55 / t0.367 / **1.65s** `gr_mg42_futz_plr`<br>f64 / t0.427 / **1.92s** `gr_mg42_mag_in_plr`<br>f101 / t0.673 / **3.03s** `gr_mg42_pull_plr`<br>f111 / t0.740 / **3.33s** `gr_mg42_charge_plr` |
| deploy | `viewmodel_mg42_deploy` | 52 f @ 30 fps = 1.73s | 1600 ms | f23 / t0.442 / **0.71s** `gr_mg_deploy_start`<br>f32 / t0.615 / **0.98s** `gr_mg_deploy_end` |
| breakdown | `viewmodel_mg42_breakdown` | 48 f @ 30 fps = 1.60s | 1200 ms | f19 / t0.396 / **0.47s** `gr_mg_break_down` |

#### `30cal_bipod`

Player sound fields: firePlr=`weap_30cal_fire_plr`, fireLastPlr=`weap_30cal_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_30cal_partial_reload` | 267 f @ 30 fps = 8.90s | 7000 ms | f13 / t0.049 / **0.34s** `gr_30cal_start_plr`<br>f15 / t0.056 / **0.39s** `gr_30cal_charge_plr`<br>f61 / t0.229 / **1.60s** `gr_30cal_open_plr`<br>f80 / t0.300 / **2.10s** `gr_30cal_grab_belt_plr`<br>f87 / t0.326 / **2.28s** `gr_30cal_belt_remove_plr`<br>f117 / t0.438 / **3.07s** `gr_30cal_belt_raise_plr`<br>f126 / t0.472 / **3.30s** `gr_30cal_belt_contact_plr`<br>f136 / t0.509 / **3.57s** `gr_30cal_belt_press_plr`<br>f152 / t0.569 / **3.99s** `gr_30cal_lid_bonk_plr`<br>f156 / t0.584 / **4.09s** `gr_30cal_close_plr`<br>f178 / t0.667 / **4.67s** `gr_30cal_tap_plr`<br>f196 / t0.734 / **5.14s** `gr_30cal_ammo_toss_plr`<br>f236 / t0.884 / **6.19s** `gr_30cal_charge_release_plr` |
| reloadEmpty | `viewmodel_30cal_reload` | 262 f @ 30 fps = 8.73s | 6000 ms | f24 / t0.092 / **0.55s** `gr_30cal_start_plr`<br>f27 / t0.103 / **0.62s** `gr_30cal_charge_plr`<br>f85 / t0.324 / **1.95s** `gr_30cal_open_plr`<br>f116 / t0.443 / **2.66s** `gr_30cal_belt_raise_plr`<br>f125 / t0.477 / **2.86s** `gr_30cal_belt_contact_plr`<br>f135 / t0.515 / **3.09s** `gr_30cal_belt_press_plr`<br>f152 / t0.580 / **3.48s** `gr_30cal_lid_bonk_plr`<br>f154 / t0.588 / **3.53s** `gr_30cal_close_plr`<br>f173 / t0.660 / **3.96s** `gr_30cal_tap_plr`<br>f193 / t0.737 / **4.42s** `gr_30cal_ammo_toss_plr`<br>f226 / t0.863 / **5.18s** `gr_30cal_charge_release_plr` |
| deploy | `viewmodel_30cal_deploy` | 48 f @ 30 fps = 1.60s | 1400 ms | f0 / t0.000 / **0.00s** `gr_30cal_start_plr`<br>f17 / t0.354 / **0.50s** `gr_mg_deploy_start`<br>f24 / t0.500 / **0.70s** `gr_mg_deploy_end` |
| breakdown | `viewmodel_30cal_breakdown` | 71 f @ 30 fps = 2.37s | 2300 ms | f0 / t0.000 / **0.00s** `gr_30cal_start_plr`<br>f18 / t0.254 / **0.58s** `gr_mg_break_down` |

#### `doublebarrel`

Player sound fields: firePlr=`weap_dbshot_fire_plr`, fireLastPlr=`weap_dbshot_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_double_barrel_shotgun_partial_reload` | 94 f @ 30 fps = 3.13s | 3000 ms | f8 / t0.085 / **0.26s** `gr_dbshot_click_plr`<br>f18 / t0.192 / **0.57s** `gr_dbshot_break_plr`<br>f40 / t0.425 / **1.28s** `gr_dbshot_shake_1brl_plr`<br>f60 / t0.638 / **1.91s** `gr_dbshot_shell_in_1_plr`<br>f80 / t0.851 / **2.55s** `gr_dbshot_close_plr` |
| reloadEmpty | `viewmodel_double_barrel_shotgun_reload` | 110 f @ 30 fps = 3.67s | 4000 ms | f5 / t0.045 / **0.18s** `gr_dbshot_click_plr`<br>f19 / t0.173 / **0.69s** `gr_dbshot_break_plr`<br>f31 / t0.282 / **1.13s** `gr_dbshot_shake_plr`<br>f66 / t0.600 / **2.40s** `gr_dbshot_shell_in_1_plr`<br>f75 / t0.682 / **2.73s** `gr_dbshot_shell_in_2_plr`<br>f95 / t0.864 / **3.45s** `gr_dbshot_close_plr` |

#### `doublebarrel_sawed_grip`

Player sound fields: firePlr=`weap_dbsawshot_fire_plr`, fireLastPlr=`weap_dbsawshot_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| rechamber | `viewmodel_trenchgun_rechamber` | 27 f @ 30 fps = 0.90s | 800 ms | f7 / t0.259 / **0.21s** `gr_shotgun_pull_plr` **(not in map)**<br>f14 / t0.518 / **0.41s** `gr_shotgun_push_plr` **(not in map)** |
| reload | `viewmodel_double_barrel_shotgun_partial_reload` | 94 f @ 30 fps = 3.13s | 3000 ms | f8 / t0.085 / **0.26s** `gr_dbshot_click_plr`<br>f18 / t0.192 / **0.57s** `gr_dbshot_break_plr`<br>f40 / t0.425 / **1.28s** `gr_dbshot_shake_1brl_plr`<br>f60 / t0.638 / **1.91s** `gr_dbshot_shell_in_1_plr`<br>f80 / t0.851 / **2.55s** `gr_dbshot_close_plr` |
| reloadEmpty | `viewmodel_double_barrel_shotgun_reload` | 110 f @ 30 fps = 3.67s | 4000 ms | f5 / t0.045 / **0.18s** `gr_dbshot_click_plr`<br>f19 / t0.173 / **0.69s** `gr_dbshot_break_plr`<br>f31 / t0.282 / **1.13s** `gr_dbshot_shake_plr`<br>f66 / t0.600 / **2.40s** `gr_dbshot_shell_in_1_plr`<br>f75 / t0.682 / **2.73s** `gr_dbshot_shell_in_2_plr`<br>f95 / t0.864 / **3.45s** `gr_dbshot_close_plr` |

#### `shotgun`

Player sound fields: firePlr=`weap_shotgun_fire_pump_plr`, fireLastPlr=`weap_shotgun_fire_pump_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| rechamber | `viewmodel_trenchgun_rechamber` | 27 f @ 30 fps = 0.90s | 800 ms | f7 / t0.259 / **0.21s** `gr_shotgun_pull_plr`<br>f14 / t0.518 / **0.41s** `gr_shotgun_push_plr` |
| reload | `viewmodel_trenchgun_reload_loop` | 39 f @ 30 fps = 1.30s | 600 ms | f17 / t0.436 / **0.26s** `gr_shotgun_shell_plr` |
| reloadEmpty | `viewmodel_trenchgun_reload_loop` | 39 f @ 30 fps = 1.30s | 600 ms | f17 / t0.436 / **0.26s** `gr_shotgun_shell_plr` |
| reloadStart | `viewmodel_trenchgun_reload_start` | 55 f @ 30 fps = 1.83s | 900 ms | f34 / t0.618 / **0.56s** `gr_shotgun_shell_plr` |
| reloadEnd | `viewmodel_trenchgun_reload_end` | 35 f @ 30 fps = 1.17s | 950 ms | f12 / t0.343 / **0.33s** `gr_shotgun_pull_plr`<br>f15 / t0.429 / **0.41s** `gr_shotgun_push_plr` |
| adsRechamber | `viewmodel_trenchgun_rechamber` | 27 f @ 30 fps = 0.90s | 800 ms | f7 / t0.259 / **0.21s** `gr_shotgun_pull_plr`<br>f14 / t0.518 / **0.41s** `gr_shotgun_push_plr` |

#### `kar98k_scoped_zombie`

Player sound fields: firePlr=`weap_kar98k_fire_plr_snp`, fireLastPlr=`weap_kar98k_fire_plr_snp`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| rechamber | `viewmodel_kar98scoped_rechamber` | 31 f @ 30 fps = 1.03s | 1200 ms | f7 / t0.226 / **0.27s** `gr_kar98_bolt_up_plr`<br>f11 / t0.355 / **0.43s** `gr_kar98_bolt_back_plr`<br>f19 / t0.613 / **0.74s** `gr_kar98_bolt_front_plr` |
| reload | `viewmodel_kar98scoped_reload_loop` | 20 f @ 30 fps = 0.67s | 600 ms | f6 / t0.300 / **0.18s** `gr_kar98_shell_in_plr` |
| reloadEmpty | `viewmodel_kar98scoped_reload_loop` | 20 f @ 30 fps = 0.67s | 600 ms | f6 / t0.300 / **0.18s** `gr_kar98_shell_in_plr` |
| reloadStart | `viewmodel_kar98scoped_reload_start` | 55 f @ 30 fps = 1.83s | 1800 ms | f4 / t0.073 / **0.13s** `gr_kar98_bolt_up_plr`<br>f7 / t0.127 / **0.23s** `gr_kar98_bolt_back_plr`<br>f41 / t0.746 / **1.34s** `gr_kar98_shell_in_plr` |
| reloadEnd | `viewmodel_kar98scoped_reload_end` | 28 f @ 30 fps = 0.93s | 770 ms | f10 / t0.357 / **0.27s** `gr_kar98_bolt_front_plr`<br>f15 / t0.536 / **0.41s** `gr_kar98_bolt_down_plr` |
| adsRechamber | `viewmodel_kar98scoped_rechamber` | 31 f @ 30 fps = 1.03s | 1200 ms | f7 / t0.226 / **0.27s** `gr_kar98_bolt_up_plr`<br>f11 / t0.355 / **0.43s** `gr_kar98_bolt_back_plr`<br>f19 / t0.613 / **0.74s** `gr_kar98_bolt_front_plr` |

#### `ptrs41_zombie`

Player sound fields: firePlr=`weap_ptrs_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, meleeHit=`melee_hit`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_ptrs_reload` | 160 f @ 30 fps = 5.33s | 4000 ms | f45 / t0.281 / **1.12s** `gr_ptrs_open_plr`<br>f92 / t0.575 / **2.30s** `gr_ptrs_futz_plr`<br>f96 / t0.600 / **2.40s** `gr_ptrs_clip_in_plr`<br>f121 / t0.756 / **3.03s** `gr_ptrs_close_plr` |
| reloadEmpty | `viewmodel_ptrs_reload_empty` | 190 f @ 30 fps = 6.33s | 5300 ms | f37 / t0.195 / **1.03s** `gr_ptrs_open_plr`<br>f76 / t0.400 / **2.12s** `gr_ptrs_futz_plr`<br>f80 / t0.421 / **2.23s** `gr_ptrs_clip_in_plr`<br>f101 / t0.532 / **2.82s** `gr_ptrs_close_plr`<br>f147 / t0.774 / **4.10s** `gr_ptrs_pull_plr`<br>f155 / t0.816 / **4.32s** `gr_ptrs_release_plr` |

#### `panzerschrek`

Player sound fields: firePlr=`weap_pnzr_fire_plr_f`, fireLastPlr=`weap_pnzr_fire_plr_f`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`, projectile=`weap_pnzr_fire_rocket`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_panzerschreck_reload` | 88 f @ 25 fps = 3.52s | 2850 ms | f2 / t0.023 / **0.06s** `gr_panzerschreck_down_plr`<br>f23 / t0.261 / **0.74s** `gr_panzerschreck_ground_plr`<br>f33 / t0.375 / **1.07s** `gr_panzerschreck_start_plr`<br>f44 / t0.500 / **1.43s** `gr_panzerschreck_tap_plr`<br>f60 / t0.682 / **1.94s** `gr_panzerschreck_up_plr` |

#### `m2_flamethrower_zombie`

Player sound fields: raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_flamethrower_foley_F`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, pickupPlr=`weap_flamethrower_foley_F`

_No sound notetracks (except the shared knife lunge)._

#### `ray_gun`

Player sound fields: firePlr=`weap_rgun_fire_plr`, fireLastPlr=`weap_rgun_fire_plr`, emptyFirePlr=`dryfire_pistol_plr`, reloadPlr=`gear_player`, reloadEmptyPlr=`gear_player`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_small_plr`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`, projectile=`weap_rgun_loop`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_ray_gun_reload` | 119 f @ 30 fps = 3.97s | 3000 ms | f12 / t0.101 / **0.30s** `ray_reload_open`<br>f39 / t0.328 / **0.98s** `ray_reload_battery_out`<br>f71 / t0.597 / **1.79s** `ray_reload_battery_in`<br>f95 / t0.798 / **2.39s** `ray_reload_close` |

#### `m7_launcher`

Player sound fields: firePlr=`weap_m1gren_fire_plr`, fireLastPlr=`weap_m1gren_fire_plr`, emptyFirePlr=`dryfire_rifle_plr`, raisePlr=`weap_raise_plr`, firstRaisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, meleeSwipePlr=`melee_swing_plr`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`

| slot | anim | anim length | WeaponDef state time | notetrack sounds: frame / normalized t / **t x state time** / alias |
|---|---|---|---|---|
| reload | `viewmodel_m1garand_grenade_reload` | 64 f @ 24 fps = 2.67s | 1500 ms | f17 / t0.266 / **0.40s** `gr_m1gren_futz`<br>f20 / t0.312 / **0.47s** `gr_m1gren_load` |
| firstRaise | `viewmodel_m1garand_grenade_first_pullout` | 70 f @ 24 fps = 2.92s | 2200 ms | f24 / t0.343 / **0.75s** `gr_m1gren_futz`<br>f33 / t0.471 / **1.04s** `gr_m1gren_load` |
| altRaise | `viewmodel_m1garand_grenade_alt_pullout` | 80 f @ 24 fps = 3.33s | 2200 ms | f32 / t0.400 / **0.88s** `gr_m1gren_load`<br>f38 / t0.475 / **1.04s** `gr_m1gren_futz` |
| altDrop | `viewmodel_m1garand_grenade_alt_putaway` | 89 f @ 24 fps = 3.71s | 2000 ms | f35 / t0.393 / **0.79s** `gr_m1gren_remove` |

#### `stielhandgranate`

Player sound fields: firePlr=`foley_throw`, raisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`, pullbackPlr=`grenade_pull_pin`

_No sound notetracks (except the shared knife lunge)._

#### `molotov`

Player sound fields: firePlr=`weap_molotov_throw`, raisePlr=`weap_raise_plr`, putawayPlr=`weap_putaway_plr`, pickupPlr=`weap_pickup_plr`, ammoPickupPlr=`ammo_pickup_plr`, pullbackPlr=`weap_molotov_light`

_No sound notetracks (except the shared knife lunge)._
