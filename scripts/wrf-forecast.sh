#!/usr/bin/env bash
# Opt-in local WRF-ARW producer. Ordinary run.sh never calls this.
# Writes working files under $XDG_CACHE_HOME/omastorm/wrf/ (or --dir).
# Wall-clock estimates live in the engine; this script prints the same
# domain numbers the estimate used and runs the Docker image when present.
set -euo pipefail

usage() {
  cat <<'EOF'
usage: wrf-forecast.sh --lat LAT --lon LON [options]

  --width-km N     Domain width (default 450)
  --height-km N    Domain height (default 450)
  --dx-km N        Grid spacing (default 15)
  --hours N        Forecast length 3–24 (default 12)
  --cores N        MPI ranks / threads (default 4)
  --nx N --ny N    WRF e_we / e_sn (defaults from span/Δx)
  --dir PATH       Work directory
  --estimate-only  Write namelists and status.json, do not start Docker
  --fetch          Download GFS 0.25° boundary files from NOMADS
EOF
  exit 2
}

lat="" lon="" width=450 height=450 dx=15 hours=12 cores=4 nx="" ny="" dir=""
estimate_only=0 fetch=0

while [[ $# -gt 0 ]]; do
  case $1 in
    --lat) lat=$2; shift 2 ;;
    --lon) lon=$2; shift 2 ;;
    --width-km) width=$2; shift 2 ;;
    --height-km) height=$2; shift 2 ;;
    --dx-km) dx=$2; shift 2 ;;
    --hours) hours=$2; shift 2 ;;
    --cores) cores=$2; shift 2 ;;
    --nx) nx=$2; shift 2 ;;
    --ny) ny=$2; shift 2 ;;
    --dir) dir=$2; shift 2 ;;
    --estimate-only) estimate_only=1; shift ;;
    --fetch) fetch=1; shift ;;
    -h|--help) usage ;;
    *) printf 'unknown option: %s\n' "$1" >&2; usage ;;
  esac
done

[[ -n $lat && -n $lon ]] || usage

if [[ -z $dir ]]; then
  cache=${XDG_CACHE_HOME:-${HOME:?}/.cache}
  dir=$cache/omastorm/wrf
fi
mkdir -p -- "$dir"

if [[ -z $nx ]]; then
  nx=$(awk -v w="$width" -v d="$dx" 'BEGIN { n=int(w/d+0.5)+1; if (n<11) n=11; if (n>201) n=201; print n }')
fi
if [[ -z $ny ]]; then
  ny=$(awk -v h="$height" -v d="$dx" 'BEGIN { n=int(h/d+0.5)+1; if (n<11) n=11; if (n>201) n=201; print n }')
fi

cells=$(( (nx-1) * (ny-1) ))
# Same CFL rule the engine uses: dt (s) ≈ 6 × Δx (km).
dt=$(awk -v d="$dx" 'BEGIN { t=6*d; if (t<6) t=6; if (t>90) t=90; printf "%.0f", t }')
steps=$(awk -v h="$hours" -v t="$dt" 'BEGIN { printf "%.0f", (h*3600)/t }')
files=$(( hours / 3 + 1 ))

# Desktop GNU WRF reference: 900 cells, 33 levels, 240 steps, 4 cores → 10 min.
integrate_min=$(awk -v c="$cells" -v s="$steps" -v n="$cores" '
  BEGIN {
    eff = (n<=4)?1.0:(n<=8)?0.85:(n<=16)?0.70:0.55
    sec = c*33*s*(600*4)/(900*33*240)/(n*eff)
    m = int(sec/60+0.999); if (m<1) m=1; print m
  }')
download_min=$(( files * 45 / 60 + 1 ))
preprocess_min=$(awk -v c="$cells" -v f="$files" 'BEGIN {
  sec=8+c/400 + 20+8*f + 15+c*f/800 + 20+c*f/1000
  m=int(sec/60+0.999); if (m<1) m=1; print m
}')
total_min=$(( download_min + preprocess_min + integrate_min ))
low=$(( total_min * 6 / 10 )); [[ $low -lt 1 ]] && low=1
high=$(( total_min * 18 / 10 + 2 ))

image=${OMASTORM_WRF_IMAGE:-ncar/wrf_tutorial:latest}

cat > "$dir/status.json" <<EOF
{"status":"running","lat":$lat,"lon":$lon,"widthKm":$width,"heightKm":$height,"dxKm":$dx,"hours":$hours,"cores":$cores,"nx":$nx,"ny":$ny,"cells":$cells,"dtSec":$dt,"steps":$steps,"gribFiles":$files,"downloadMin":$download_min,"preprocessMin":$preprocess_min,"integrateMin":$integrate_min,"totalMin":$total_min,"totalMinLow":$low,"totalMinHigh":$high,"image":"$image"}
EOF

start=$(date -u +%Y-%m-%d_%H:00:00)
end=$(date -u -d "+${hours} hours" +%Y-%m-%d_%H:00:00 2>/dev/null || date -u -v+"${hours}"H +%Y-%m-%d_%H:00:00)

proj=lambert
alat=$(awk -v a="$lat" 'BEGIN { if (a<0) print -a; else print a }')
if awk -v a="$alat" 'BEGIN { exit !(a<30) }'; then proj=mercator; fi
if awk -v a="$alat" 'BEGIN { exit !(a>60) }'; then proj=polar; fi

cat > "$dir/namelist.wps" <<EOF
&share
 wrf_core = 'ARW',
 max_dom = 1,
 start_date = '$start',
 end_date = '$end',
 interval_seconds = 10800,
 io_form_geogrid = 2,
/
&geogrid
 parent_id         = 1,
 parent_grid_ratio = 1,
 i_parent_start    = 1,
 j_parent_start    = 1,
 e_we              = $nx,
 e_sn              = $ny,
 geog_data_res     = 'lowres',
 dx = $(awk -v d="$dx" 'BEGIN { printf "%.0f", d*1000 }'),
 dy = $(awk -v d="$dx" 'BEGIN { printf "%.0f", d*1000 }'),
 map_proj = '$proj',
 ref_lat   = $lat,
 ref_lon   = $lon,
 truelat1  = $lat,
 truelat2  = $lat,
 stand_lon = $lon,
 geog_data_path = '${OMASTORM_WRF_GEOG:-/wrf/WPS_GEOG}',
/
&ungrib
 out_format = 'WPS',
 prefix = 'FILE',
/
&metgrid
 fg_name = 'FILE',
 io_form_metgrid = 2,
/
EOF

printf 'WRF domain %s×%s km at %s km → %sx%s cells, dt %ss, %s steps, %s GFS files.\n' \
  "$width" "$height" "$dx" "$((nx-1))" "$((ny-1))" "$dt" "$steps" "$files"
printf 'Estimate %s–%s min (download ~%s, WPS/real ~%s, wrf.exe ~%s) on %s cores.\n' \
  "$low" "$high" "$download_min" "$preprocess_min" "$integrate_min" "$cores"

if [[ $estimate_only -eq 1 ]]; then
  printf 'estimate-only: namelist and status written to %s\n' "$dir"
  exit 0
fi

if ! command -v docker >/dev/null 2>&1; then
  printf 'docker is not installed; set OMASTORM_WRF_IMAGE after installing Docker.\n' >&2
  exit 3
fi

if [[ $fetch -eq 1 ]]; then
  mkdir -p -- "$dir/grib"
  cycle=$(date -u +%Y%m%d)
  hour=$(date -u +%H)
  case $hour in
    0[0-5]) hh=00 ;; 0[6-9]|1[0-1]) hh=06 ;; 1[2-7]) hh=12 ;; *) hh=18 ;;
  esac
  # Previous cycle is more likely to be complete on NOMADS.
  if [[ $hh == 00 ]]; then
    cycle=$(date -u -d yesterday +%Y%m%d 2>/dev/null || date -u -v-1d +%Y%m%d)
    hh=18
  fi
  f=0
  while [[ $f -le $hours ]]; do
    ff=$(printf '%03d' "$f")
    url="https://nomads.ncep.noaa.gov/pub/data/nccf/com/gfs/prod/gfs.${cycle}/${hh}/atmos/gfs.t${hh}z.pgrb2.0p25.f${ff}"
    dest=$dir/grib/gfs.t${hh}z.pgrb2.0p25.f${ff}
    if [[ ! -s $dest ]]; then
      printf 'fetch %s\n' "$url"
      curl -fsSL --retry 2 --retry-delay 2 -o "$dest" "$url" || true
    fi
    f=$((f+3))
  done
fi

# The image is expected to provide WPS + WRF. Mount the work directory and run
# the usual geogrid/ungrib/metgrid/real/wrf sequence when those binaries exist.
docker run --rm \
  --user "$(id -u):$(id -g)" \
  --cpus="$cores" \
  -e OMP_NUM_THREADS="$cores" \
  -v "$dir:/wrf/omastorm" \
  ${OMASTORM_WRF_GEOG:+-v "$OMASTORM_WRF_GEOG:/wrf/WPS_GEOG:ro"} \
  "$image" \
  /bin/sh -c '
    set -e
    cp /wrf/omastorm/namelist.wps /wrf/WPS/namelist.wps 2>/dev/null || cp /wrf/omastorm/namelist.wps namelist.wps
    cd /wrf/WPS 2>/dev/null || cd /WPS || true
    if command -v ./geogrid.exe >/dev/null; then
      ./geogrid.exe
      ln -sf ungrib/Variable_Tables/Vtable.GFS Vtable
      ./link_grib.csh /wrf/omastorm/grib/gfs* || true
      ./ungrib.exe
      ./metgrid.exe
    fi
    cd /wrf/WRF/run 2>/dev/null || cd /WRF/run || true
    if command -v ./real.exe >/dev/null; then
      ./real.exe
      ./wrf.exe
      cp wrfout_* /wrf/omastorm/ 2>/dev/null || true
    fi
  '
