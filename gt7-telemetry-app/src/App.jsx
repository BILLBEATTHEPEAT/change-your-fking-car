import { useCallback, useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import ReactECharts from 'echarts-for-react'
import './App.css'

function App() {
  const [status, setStatus] = useState({
    version: '0.0.0',
    listenerRunning: false,
    lastPacketAt: null,
    packetCount: 0,
    lastPacketMeta: null,
    dbPath: null,
    lastSample: null,
    targetIp: null,
    lastHeartbeatAt: null,
    lastListenerError: null,
    boundPorts: [],
    currentSessionId: null,
  })
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [targetIp, setTargetIp] = useState('')
  const [livePayload, setLivePayload] = useState(null)
  const [laps, setLaps] = useState([])
  const [referenceLapId, setReferenceLapId] = useState('')
  const [compareLapId, setCompareLapId] = useState('')
  const [deleteLapId, setDeleteLapId] = useState('')
  const [referenceSamples, setReferenceSamples] = useState([])
  const [compareSamples, setCompareSamples] = useState([])
  const [trackPoints, setTrackPoints] = useState([])
  const [smoothLines, setSmoothLines] = useState(true)
  const [showLegends, setShowLegends] = useState(true)
  const [raceLineColorMode, setRaceLineColorMode] = useState('delta')
  const [currentSessionId, setCurrentSessionId] = useState(null)
  const [showPeaksValleys, setShowPeaksValleys] = useState(true)
  const [peakThreshold, setPeakThreshold] = useState(4)
  const [peakSpacing, setPeakSpacing] = useState(6)
  const [exportLimit, setExportLimit] = useState(1200)
  const [importStatus, setImportStatus] = useState('')
  const [peakPreset, setPeakPreset] = useState('balanced')
  const [importPreview, setImportPreview] = useState(null)
  const [importFile, setImportFile] = useState(null)
  const [sessions, setSessions] = useState([])
  const [dbInfo, setDbInfo] = useState(null)
  const [lastLapId, setLastLapId] = useState(null)
  const [detailedLaps, setDetailedLaps] = useState([])
  const [varianceData, setVarianceData] = useState(null)
  const [showVariance, setShowVariance] = useState(false)
  const [varianceLapCount, setVarianceLapCount] = useState(5)
  const [fuelAnalysis, setFuelAnalysis] = useState(null)
  const [medianLapId, setMedianLapId] = useState(null)
  const [replayLaps, setReplayLaps] = useState([])
  const [replayFilter, setReplayFilter] = useState('all') // 'all', 'replays', 'live'

  const lastPacketLabel = useMemo(() => {
    if (!status.lastPacketAt) return 'No packets yet'
    const date = new Date(status.lastPacketAt)
    return date.toLocaleString()
  }, [status.lastPacketAt])

  const formatLapTime = useCallback((ms) => {
    if (!ms || ms <= 0) return '—'
    const totalSeconds = Math.floor(ms / 1000)
    const minutes = Math.floor(totalSeconds / 60)
    const seconds = totalSeconds % 60
    const millis = ms % 1000
    return `${minutes}:${String(seconds).padStart(2, '0')}.${String(millis).padStart(3, '0')}`
  }, [])

  const formatDuration = useCallback((ms) => {
    if (!ms || ms <= 0) return '—'
    const totalSeconds = Math.floor(ms / 1000)
    const minutes = Math.floor(totalSeconds / 60)
    const seconds = totalSeconds % 60
    return `${minutes}m ${String(seconds).padStart(2, '0')}s`
  }, [])

  const buildDistanceSeries = useCallback((samples) => {
    if (!samples || samples.length === 0) return []
    let distance = 0
    let lastTs = samples[0].tsMs
    const series = []

    for (const sample of samples) {
      const dt = Math.max(0, (sample.tsMs - lastTs) / 1000)
      const speedMps = sample.speedKmh / 3.6
      distance += speedMps * dt
      series.push({
        distance,
        speedKmh: sample.speedKmh,
        throttle: sample.throttle,
        brake: sample.brake,
        rpm: sample.rpm,
      })
      lastTs = sample.tsMs
    }

    return series
  }, [])

  const detectPeaksValleys = useCallback((series, distances) => {
    if (series.length < 5) return { peaks: [], valleys: [] }
    const window = Math.max(2, Math.round(peakSpacing))
    const threshold = Math.max(1, Math.round(peakThreshold))
    const peaks = []
    const valleys = []

    for (let i = window; i < series.length - window; i += 1) {
      const slice = series.slice(i - window, i + window + 1)
      const value = series[i]
      const max = Math.max(...slice)
      const min = Math.min(...slice)

      if (value === max && value - min >= threshold) {
        peaks.push([distances[i], value])
      }
      if (value === min && max - value >= threshold) {
        valleys.push([distances[i], value])
      }
    }

    return {
      peaks: peaks.filter((_, idx) => idx % Math.max(1, Math.round(peakSpacing)) === 0),
      valleys: valleys.filter((_, idx) => idx % Math.max(1, Math.round(peakSpacing)) === 0),
    }
  }, [peakSpacing, peakThreshold])

  const applyPeakPreset = useCallback((preset) => {
    setPeakPreset(preset)
    if (preset === 'aggressive') {
      setPeakThreshold(2)
      setPeakSpacing(4)
    } else if (preset === 'smooth') {
      setPeakThreshold(6)
      setPeakSpacing(10)
    } else {
      setPeakThreshold(4)
      setPeakSpacing(6)
    }
    setShowPeaksValleys(true)
  }, [])

  const resampleByDistance = useCallback((series, points) => {
    if (!series.length) return []
    const total = series[series.length - 1].distance
    if (total <= 0) return []
    const step = total / (points - 1)
    const resampled = []
    let idx = 0

    for (let i = 0; i < points; i += 1) {
      const target = step * i
      while (idx < series.length - 1 && series[idx + 1].distance < target) {
        idx += 1
      }
      const a = series[idx]
      const b = series[Math.min(idx + 1, series.length - 1)]
      const span = b.distance - a.distance
      const t = span > 0 ? (target - a.distance) / span : 0
      resampled.push({
        distance: target,
        speedKmh: a.speedKmh + (b.speedKmh - a.speedKmh) * t,
        throttle: a.throttle + (b.throttle - a.throttle) * t,
        brake: a.brake + (b.brake - a.brake) * t,
        rpm: a.rpm + (b.rpm - a.rpm) * t,
      })
    }

    return resampled
  }, [])

  const decimate = useCallback((series, maxPoints) => {
    if (series.length <= maxPoints) return series
    const step = Math.ceil(series.length / maxPoints)
    const result = []
    for (let i = 0; i < series.length; i += step) {
      result.push(series[i])
    }
    return result
  }, [])

  const refreshStatus = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const nextStatus = await invoke('get_app_status')
      setStatus(nextStatus)
      if (nextStatus.currentSessionId && nextStatus.currentSessionId !== currentSessionId) {
        setCurrentSessionId(nextStatus.currentSessionId)
      }
      if (nextStatus.targetIp && !targetIp) {
        setTargetIp(nextStatus.targetIp)
      }
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load status')
    } finally {
      setLoading(false)
    }
  }, [targetIp, currentSessionId])

  const toggleListener = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const command = status.listenerRunning ? 'stop_listener' : 'start_listener'
      const nextStatus = await invoke(command)
      setStatus(nextStatus)
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to update listener')
    } finally {
      setLoading(false)
    }
  }, [status.listenerRunning])

  const saveTargetIp = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const nextStatus = await invoke('set_target_ip', { ip: targetIp })
      setStatus(nextStatus)
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to save IP')
    } finally {
      setLoading(false)
    }
  }, [targetIp])

  const loadLivePayload = useCallback(async () => {
    try {
      const payload = await invoke('get_live_payload')
      setLivePayload(payload)
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load live payload')
    }
  }, [])

  const loadDatabaseInfo = useCallback(async () => {
    try {
      const info = await invoke('get_database_info')
      setDbInfo(info)
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load database info')
    }
  }, [])

  const loadLaps = useCallback(async () => {
    try {
      const data = await invoke('list_laps', {
        sessionId: currentSessionId ? Number(currentSessionId) : null,
      })
      setLaps(data)
      // Track last lap
      const lastLap = data.find(l => l.isLastLap)
      if (lastLap) {
        setLastLapId(String(lastLap.id))
      }
      if (!referenceLapId && data.length > 0) {
        setReferenceLapId(String(data[0].id))
      }
      if (!compareLapId && data.length > 1) {
        setCompareLapId(String(data[1].id))
      }
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load laps')
    }
  }, [referenceLapId, compareLapId, currentSessionId])

  const loadDetailedLaps = useCallback(async () => {
    if (!currentSessionId) return
    try {
      const data = await invoke('list_laps_detailed', { sessionId: Number(currentSessionId) })
      setDetailedLaps(data)
      const lastLap = data.find(l => l.isLastLap)
      if (lastLap) {
        setLastLapId(String(lastLap.id))
      }
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load detailed laps')
    }
  }, [currentSessionId])

  const loadFuelAnalysis = useCallback(async () => {
    if (!currentSessionId) return
    try {
      const data = await invoke('get_session_fuel_analysis', { sessionId: Number(currentSessionId) })
      setFuelAnalysis(data)
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load fuel analysis')
    }
  }, [currentSessionId])

  const loadVarianceData = useCallback(async () => {
    if (!currentSessionId) return
    try {
      const bestLaps = await invoke('get_best_laps', {
        sessionId: Number(currentSessionId),
        count: varianceLapCount,
      })
      if (bestLaps.length >= 2) {
        const lapIds = bestLaps.map(l => l.id)
        const variance = await invoke('get_speed_variance', {
          lapIds,
          points: 200,
        })
        setVarianceData(variance)
      }
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load variance data')
    }
  }, [currentSessionId, varianceLapCount])

  const loadMedianLap = useCallback(async () => {
    if (!currentSessionId) return
    try {
      const medianLap = await invoke('get_median_lap', { sessionId: Number(currentSessionId) })
      if (medianLap) {
        setMedianLapId(String(medianLap.id))
      }
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load median lap')
    }
  }, [currentSessionId])

  const loadReplayLaps = useCallback(async () => {
    if (!currentSessionId) return
    try {
      const data = await invoke('list_replay_laps', { sessionId: Number(currentSessionId) })
      setReplayLaps(data)
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load replay laps')
    }
  }, [currentSessionId])

  const useLastLap = useCallback(() => {
    if (lastLapId) {
      setCompareLapId(lastLapId)
    }
  }, [lastLapId])

  const useMedianLap = useCallback(() => {
    if (medianLapId) {
      setCompareLapId(medianLapId)
    }
  }, [medianLapId])

  const loadSessions = useCallback(async () => {
    try {
      const data = await invoke('list_sessions')
      setSessions(data)
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load sessions')
    }
  }, [])

  const loadSessionPreferences = useCallback(async () => {
    try {
      const prefs = await invoke('get_session_preferences')
      if (prefs.referenceLapId) {
        setReferenceLapId(String(prefs.referenceLapId))
      }
      if (prefs.compareLapId) {
        setCompareLapId(String(prefs.compareLapId))
      }
      if (prefs.smoothLines !== null && prefs.smoothLines !== undefined) {
        setSmoothLines(Boolean(prefs.smoothLines))
      }
      if (prefs.showLegends !== null && prefs.showLegends !== undefined) {
        setShowLegends(Boolean(prefs.showLegends))
      }
      if (prefs.raceLineColorMode) {
        setRaceLineColorMode(prefs.raceLineColorMode)
      }
      if (prefs.showPeaks !== null && prefs.showPeaks !== undefined) {
        setShowPeaksValleys(Boolean(prefs.showPeaks))
      }
      if (prefs.peakThreshold !== null && prefs.peakThreshold !== undefined) {
        setPeakThreshold(Number(prefs.peakThreshold))
      }
      if (prefs.peakSpacing !== null && prefs.peakSpacing !== undefined) {
        setPeakSpacing(Number(prefs.peakSpacing))
      }
      if (prefs.peakThreshold || prefs.peakSpacing) {
        if (prefs.peakThreshold <= 3) {
          setPeakPreset('aggressive')
        } else if (prefs.peakThreshold >= 6) {
          setPeakPreset('smooth')
        } else {
          setPeakPreset('balanced')
        }
      }
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to load session preferences')
    }
  }, [])

  const saveSessionPreferences = useCallback(
    async (nextReferenceId, nextCompareId) => {
      try {
        await invoke('set_session_preferences', {
          referenceLapId: nextReferenceId ? Number(nextReferenceId) : null,
          compareLapId: nextCompareId ? Number(nextCompareId) : null,
          smoothLines,
          showLegends,
          raceLineColorMode,
          showPeaks: showPeaksValleys,
          peakThreshold: Math.round(peakThreshold),
          peakSpacing: Math.round(peakSpacing),
        })
      } catch (err) {
        setError(err?.toString?.() ?? 'Failed to save session preferences')
      }
    },
    [smoothLines, showLegends, raceLineColorMode, showPeaksValleys, peakThreshold, peakSpacing],
  )

  const initDatabase = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      await invoke('init_database')
      await refreshStatus()
      await loadLaps()
      await loadSessions()
      await loadDatabaseInfo()
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to initialize database')
    } finally {
      setLoading(false)
    }
  }, [refreshStatus, loadLaps, loadSessions, loadDatabaseInfo])

  const vacuumDatabase = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      await invoke('vacuum_database')
      await loadDatabaseInfo()
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to vacuum database')
    } finally {
      setLoading(false)
    }
  }, [loadDatabaseInfo])

  const resetDatabase = useCallback(async () => {
    if (!window.confirm('Delete the local database file? This cannot be undone.')) return
    setLoading(true)
    setError('')
    try {
      await invoke('reset_database')
      await refreshStatus()
      await loadSessions()
      await loadLaps()
      await loadDatabaseInfo()
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to reset database')
    } finally {
      setLoading(false)
    }
  }, [refreshStatus, loadSessions, loadLaps, loadDatabaseInfo])

  const deleteSession = useCallback(async () => {
    if (!currentSessionId) return
    if (!window.confirm(`Delete session ${currentSessionId}? This cannot be undone.`)) return
    setLoading(true)
    setError('')
    try {
      await invoke('delete_session', { sessionId: Number(currentSessionId) })
      setCurrentSessionId(null)
      setReferenceLapId('')
      setCompareLapId('')
      await loadSessions()
      await loadLaps()
      await loadDatabaseInfo()
      await refreshStatus()
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to delete session')
    } finally {
      setLoading(false)
    }
  }, [currentSessionId, loadSessions, loadLaps, loadDatabaseInfo, refreshStatus])

  const deleteLap = useCallback(async () => {
    if (!deleteLapId) return
    if (!window.confirm(`Delete lap ${deleteLapId}? This cannot be undone.`)) return
    setLoading(true)
    setError('')
    try {
      await invoke('delete_lap', { lapId: Number(deleteLapId) })
      if (referenceLapId === deleteLapId) setReferenceLapId('')
      if (compareLapId === deleteLapId) setCompareLapId('')
      setDeleteLapId('')
      await loadLaps()
      await loadDatabaseInfo()
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to delete lap')
    } finally {
      setLoading(false)
    }
  }, [deleteLapId, referenceLapId, compareLapId, loadLaps, loadDatabaseInfo])

  const selectSession = useCallback(
    async (sessionId) => {
      if (!sessionId) return
      setLoading(true)
      setError('')
      try {
        await invoke('set_current_session', { sessionId: Number(sessionId) })
        setCurrentSessionId(Number(sessionId))
        await loadSessionPreferences()
        await loadLaps()
      } catch (err) {
        setError(err?.toString?.() ?? 'Failed to switch session')
      } finally {
        setLoading(false)
      }
    },
    [loadSessionPreferences, loadLaps],
  )


  useEffect(() => {
    refreshStatus()
  }, [refreshStatus])

  useEffect(() => {
    const interval = setInterval(() => {
      refreshStatus()
    }, 1000)

    return () => clearInterval(interval)
  }, [refreshStatus])

  useEffect(() => {
    loadDatabaseInfo()
  }, [loadDatabaseInfo])

  useEffect(() => {
    if (!status.listenerRunning) return undefined
    const interval = setInterval(() => {
      loadLivePayload()
    }, 200)

    return () => clearInterval(interval)
  }, [loadLivePayload, status.listenerRunning])

  useEffect(() => {
    const interval = setInterval(() => {
      loadLaps()
      loadDetailedLaps()
      loadSessions()
      loadDatabaseInfo()
    }, 2000)

    return () => clearInterval(interval)
  }, [loadLaps, loadDetailedLaps, loadSessions, loadDatabaseInfo])

  useEffect(() => {
    if (!currentSessionId) return
    loadSessionPreferences()
    loadMedianLap()
    loadFuelAnalysis()
  }, [currentSessionId, loadSessionPreferences, loadMedianLap, loadFuelAnalysis])

  useEffect(() => {
    if (!currentSessionId || !showVariance) return
    loadVarianceData()
  }, [currentSessionId, showVariance, varianceLapCount, loadVarianceData])

  useEffect(() => {
    if (!currentSessionId) return
    loadReplayLaps()
  }, [currentSessionId, loadReplayLaps])

  useEffect(() => {
    if (!currentSessionId) return
    loadSessionPreferences()
  }, [currentSessionId, loadSessionPreferences])

  useEffect(() => {
    if (!currentSessionId) return
    saveSessionPreferences(referenceLapId, compareLapId)
  }, [
    currentSessionId,
    referenceLapId,
    compareLapId,
    smoothLines,
    showLegends,
    raceLineColorMode,
    showPeaksValleys,
    peakThreshold,
    peakSpacing,
    saveSessionPreferences,
  ])

  useEffect(() => {
    const loadLapData = async () => {
      if (!referenceLapId) return
      try {
        const samples = await invoke('get_lap_samples', {
          lapId: Number(referenceLapId),
          limit: 1500,
        })
        setReferenceSamples(samples)
        const points = await invoke('get_lap_track_points', {
          lapId: Number(referenceLapId),
          limit: 1500,
        })
        setTrackPoints(points)
      } catch (err) {
        setError(err?.toString?.() ?? 'Failed to load reference lap data')
      }
    }

    loadLapData()
  }, [referenceLapId])

  useEffect(() => {
    const loadCompareData = async () => {
      if (!compareLapId) return
      try {
        const samples = await invoke('get_lap_samples', {
          lapId: Number(compareLapId),
          limit: 1500,
        })
        setCompareSamples(samples)
      } catch (err) {
        setError(err?.toString?.() ?? 'Failed to load compare lap data')
      }
    }

    loadCompareData()
  }, [compareLapId])

  const comparisonOption = useMemo(() => {
    const refSeries = resampleByDistance(buildDistanceSeries(referenceSamples), 450)
    const cmpSeries = resampleByDistance(buildDistanceSeries(compareSamples), 450)
    const maxLen = Math.max(refSeries.length, cmpSeries.length)
    const distanceAxis = Array.from({ length: maxLen }, (_, i) => {
      const value = refSeries[i]?.distance ?? cmpSeries[i]?.distance ?? 0
      return Math.round(value)
    })

    const refSpeed = refSeries.map((point) => point.speedKmh)
    const cmpSpeed = cmpSeries.map((point) => point.speedKmh)
    const deltaSpeed = refSeries.map((point, idx) => {
      const compare = cmpSeries[idx]
      if (!compare) return null
      return point.speedKmh - compare.speedKmh
    })

    const refThrottle = refSeries.map((point) => point.throttle)
    const cmpThrottle = cmpSeries.map((point) => point.throttle)
    const refBrake = refSeries.map((point) => point.brake)
    const cmpBrake = cmpSeries.map((point) => point.brake)

    const { peaks, valleys } = detectPeaksValleys(refSpeed, distanceAxis)

    return {
      tooltip: { trigger: 'axis' },
      legend: showLegends ? { data: ['Reference', 'Compare', 'Delta', 'Peaks', 'Valleys'] } : undefined,
      grid: { left: 40, right: 30, top: 40, bottom: 30 },
      xAxis: { type: 'category', data: distanceAxis, name: 'm', axisLabel: { show: false } },
      yAxis: [
        { type: 'value', name: 'km/h' },
        { type: 'value', name: 'Δ km/h' },
      ],
      series: [
        {
          name: 'Reference',
          type: 'line',
          data: refSpeed,
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Compare',
          type: 'line',
          data: cmpSpeed,
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Delta',
          type: 'line',
          yAxisIndex: 1,
          data: deltaSpeed,
          smooth: smoothLines,
          showSymbol: false,
        },
        ...(showPeaksValleys
          ? [
              {
                name: 'Peaks',
                type: 'scatter',
                data: peaks,
                symbolSize: 6,
                itemStyle: { color: '#f59e0b' },
              },
              {
                name: 'Valleys',
                type: 'scatter',
                data: valleys,
                symbolSize: 6,
                itemStyle: { color: '#2563eb' },
              },
            ]
          : []),
      ],
    }
  }, [
    referenceSamples,
    compareSamples,
    buildDistanceSeries,
    resampleByDistance,
    showLegends,
    smoothLines,
    detectPeaksValleys,
    showPeaksValleys,
  ])

  const throttleBrakeOption = useMemo(() => {
    const refSeries = resampleByDistance(buildDistanceSeries(referenceSamples), 450)
    const cmpSeries = resampleByDistance(buildDistanceSeries(compareSamples), 450)
    const maxLen = Math.max(refSeries.length, cmpSeries.length)
    const distanceAxis = Array.from({ length: maxLen }, (_, i) => {
      const value = refSeries[i]?.distance ?? cmpSeries[i]?.distance ?? 0
      return Math.round(value)
    })

    return {
      tooltip: { trigger: 'axis' },
      legend: showLegends ? { data: ['Ref Throttle', 'Ref Brake', 'Cmp Throttle', 'Cmp Brake'] } : undefined,
      grid: { left: 40, right: 20, top: 40, bottom: 30 },
      xAxis: { type: 'category', data: distanceAxis, name: 'm', axisLabel: { show: false } },
      yAxis: { type: 'value', name: '%', max: 100 },
      series: [
        {
          name: 'Ref Throttle',
          type: 'line',
          data: refSeries.map((point) => point.throttle),
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Ref Brake',
          type: 'line',
          data: refSeries.map((point) => point.brake),
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Cmp Throttle',
          type: 'line',
          data: cmpSeries.map((point) => point.throttle),
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Cmp Brake',
          type: 'line',
          data: cmpSeries.map((point) => point.brake),
          smooth: smoothLines,
          showSymbol: false,
        },
      ],
    }
  }, [referenceSamples, compareSamples, buildDistanceSeries, resampleByDistance, showLegends, smoothLines])

  const rpmOption = useMemo(() => {
    const refSeries = resampleByDistance(buildDistanceSeries(referenceSamples), 450)
    const cmpSeries = resampleByDistance(buildDistanceSeries(compareSamples), 450)
    const maxLen = Math.max(refSeries.length, cmpSeries.length)
    const distanceAxis = Array.from({ length: maxLen }, (_, i) => {
      const value = refSeries[i]?.distance ?? cmpSeries[i]?.distance ?? 0
      return Math.round(value)
    })

    const deltaRpm = refSeries.map((point, idx) => {
      const compare = cmpSeries[idx]
      if (!compare) return null
      return point.rpm - compare.rpm
    })

    return {
      tooltip: { trigger: 'axis' },
      legend: showLegends ? { data: ['Ref RPM', 'Cmp RPM', 'Δ RPM'] } : undefined,
      grid: { left: 40, right: 20, top: 40, bottom: 30 },
      xAxis: { type: 'category', data: distanceAxis, name: 'm', axisLabel: { show: false } },
      yAxis: [
        { type: 'value', name: 'RPM' },
        { type: 'value', name: 'Δ RPM' },
      ],
      series: [
        {
          name: 'Ref RPM',
          type: 'line',
          data: refSeries.map((point) => point.rpm),
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Cmp RPM',
          type: 'line',
          data: cmpSeries.map((point) => point.rpm),
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Δ RPM',
          type: 'line',
          yAxisIndex: 1,
          data: deltaRpm,
          smooth: smoothLines,
          showSymbol: false,
        },
      ],
    }
  }, [referenceSamples, compareSamples, buildDistanceSeries, resampleByDistance, showLegends, smoothLines])

  const buildChartData = useCallback((series) => {
    return {
      distance: series.map((point) => Math.round(point.distance)),
      speed: series.map((point) => point.speedKmh),
      throttle: series.map((point) => point.throttle),
      brake: series.map((point) => point.brake),
      rpm: series.map((point) => point.rpm),
    }
  }, [])

  const splitViewOption = useMemo(() => {
    const refSeries = resampleByDistance(buildDistanceSeries(referenceSamples), 450)
    const cmpSeries = resampleByDistance(buildDistanceSeries(compareSamples), 450)
    const ref = buildChartData(refSeries)
    const cmp = buildChartData(cmpSeries)
    const maxLen = Math.max(ref.distance.length, cmp.distance.length)
    const distanceAxis = Array.from({ length: maxLen }, (_, i) => {
      return ref.distance[i] ?? cmp.distance[i] ?? 0
    })

    return {
      tooltip: { trigger: 'axis' },
      legend: showLegends ? { data: ['Ref Speed', 'Ref Throttle', 'Ref Brake', 'Cmp Speed', 'Cmp Throttle', 'Cmp Brake'] } : undefined,
      axisPointer: {
        link: [{ xAxisIndex: 'all' }],
      },
      grid: [
        { left: 50, right: 30, top: 40, height: 120 },
        { left: 50, right: 30, top: 200, height: 120 },
      ],
      xAxis: [
        { type: 'category', data: distanceAxis, axisLabel: { show: false } },
        { type: 'category', data: distanceAxis, axisLabel: { show: false }, gridIndex: 1 },
      ],
      yAxis: [
        { type: 'value', name: 'km/h' },
        { type: 'value', name: '%', max: 100, gridIndex: 1 },
      ],
      series: [
        {
          name: 'Ref Speed',
          type: 'line',
          data: ref.speed,
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Cmp Speed',
          type: 'line',
          data: cmp.speed,
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Ref Throttle',
          type: 'line',
          xAxisIndex: 1,
          yAxisIndex: 1,
          data: ref.throttle,
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Ref Brake',
          type: 'line',
          xAxisIndex: 1,
          yAxisIndex: 1,
          data: ref.brake,
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Cmp Throttle',
          type: 'line',
          xAxisIndex: 1,
          yAxisIndex: 1,
          data: cmp.throttle,
          smooth: smoothLines,
          showSymbol: false,
        },
        {
          name: 'Cmp Brake',
          type: 'line',
          xAxisIndex: 1,
          yAxisIndex: 1,
          data: cmp.brake,
          smooth: smoothLines,
          showSymbol: false,
        },
      ],
    }
  }, [referenceSamples, compareSamples, buildDistanceSeries, resampleByDistance, buildChartData, smoothLines, showLegends])

  const exportSession = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const data = await invoke('export_session_snapshot', {
        maxSamplesPerLap: Math.max(100, Number(exportLimit) || 1200),
      })
      const blob = new Blob([data], { type: 'application/json' })
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = `gt7-session-${currentSessionId ?? 'latest'}.json`
      link.click()
      URL.revokeObjectURL(url)
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to export session')
    } finally {
      setLoading(false)
    }
  }, [exportLimit, currentSessionId])

  const previewImport = useCallback(async (file) => {
    if (!file) return
    setLoading(true)
    setError('')
    setImportStatus('')
    try {
      const text = await file.text()
      const parsed = JSON.parse(text)
      setImportFile({ name: file.name, text })
      setImportPreview({
        lapCount: parsed?.laps?.length ?? 0,
        preferences: parsed?.preferences ?? {},
      })
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to parse import file')
    } finally {
      setLoading(false)
    }
  }, [])

  const confirmImport = useCallback(async () => {
    if (!importFile?.text) return
    setLoading(true)
    setError('')
    try {
      const sessionId = await invoke('import_session_snapshot', { json: importFile.text })
      setImportStatus(`Imported session ${sessionId}`)
      setImportPreview(null)
      setImportFile(null)
      setCurrentSessionId(sessionId)
      await refreshStatus()
      await loadSessions()
      await loadLaps()
      await loadSessionPreferences()
      await loadDatabaseInfo()
    } catch (err) {
      setError(err?.toString?.() ?? 'Failed to import session')
    } finally {
      setLoading(false)
    }
  }, [importFile, refreshStatus, loadSessions, loadLaps, loadSessionPreferences, loadDatabaseInfo])

  const trackOption = useMemo(() => {
    const refSeries = buildDistanceSeries(referenceSamples)
    const cmpSeries = buildDistanceSeries(compareSamples)
    const alignedCompare = resampleByDistance(cmpSeries, refSeries.length)

    const distanceAxis = refSeries.map((point) => point.distance)
    const { peaks, valleys } = detectPeaksValleys(
      refSeries.map((point) => point.speedKmh),
      distanceAxis,
    )

    const mapDistanceToIndex = (distance) => {
      if (!distanceAxis.length) return 0
      let low = 0
      let high = distanceAxis.length - 1
      while (low < high) {
        const mid = Math.floor((low + high) / 2)
        if (distanceAxis[mid] < distance) {
          low = mid + 1
        } else {
          high = mid
        }
      }
      return low
    }

    const trackWithIndex = trackPoints.map((point, idx) => ({
      ...point,
      idx,
    }))
    const decimatedTrack = decimate(trackWithIndex, 1200)
    const throttlePoints = decimatedTrack
      .filter((point) => point.throttle > 5)
      .map((point) => [point.x, point.z, point.throttle])
    const brakePoints = decimatedTrack
      .filter((point) => point.brake > 5)
      .map((point) => [point.x, point.z, point.brake])

    const peakMarkers = peaks
      .map(([distance]) => {
        const idx = mapDistanceToIndex(distance)
        if (!trackPoints.length) return null
        const trackIdx = Math.round((idx / Math.max(1, refSeries.length - 1)) * (trackPoints.length - 1))
        const point = trackPoints[trackIdx]
        return point ? [point.x, point.z] : null
      })
      .filter(Boolean)

    const valleyMarkers = valleys
      .map(([distance]) => {
        const idx = mapDistanceToIndex(distance)
        if (!trackPoints.length) return null
        const trackIdx = Math.round((idx / Math.max(1, refSeries.length - 1)) * (trackPoints.length - 1))
        const point = trackPoints[trackIdx]
        return point ? [point.x, point.z] : null
      })
      .filter(Boolean)

    const peakIndexSet = new Set(
      peaks.map(([distance]) =>
        Math.round((mapDistanceToIndex(distance) / Math.max(1, refSeries.length - 1)) * (trackPoints.length - 1)),
      ),
    )
    const valleyIndexSet = new Set(
      valleys.map(([distance]) =>
        Math.round((mapDistanceToIndex(distance) / Math.max(1, refSeries.length - 1)) * (trackPoints.length - 1)),
      ),
    )

    const coloredPoints = decimatedTrack.map((point) => {
      const ref = refSeries[point.idx]
      const cmp = alignedCompare[point.idx]
      const delta = ref && cmp ? ref.speedKmh - cmp.speedKmh : 0
      const throttle = point.throttle
      const brake = point.brake

      let value = delta
      if (raceLineColorMode === 'throttle') {
        value = throttle
      } else if (raceLineColorMode === 'brake') {
        value = brake
      } else if (raceLineColorMode === 'peaks') {
        const trackIdx = Math.round((point.idx / Math.max(1, refSeries.length - 1)) * (trackPoints.length - 1))
        if (peakIndexSet.has(trackIdx)) {
          value = 1
        } else if (valleyIndexSet.has(trackIdx)) {
          value = -1
        } else {
          value = 0
        }
      }

      return [point.x, point.z, value]
    })

    const values = coloredPoints.map((point) => point[2])
    const minValue = values.length ? Math.min(...values) : -20
    const maxValue = values.length ? Math.max(...values) : 20

    const colorRange =
      raceLineColorMode === 'delta'
        ? ['#b91c1c', '#fbbf24', '#0f766e']
        : raceLineColorMode === 'throttle'
          ? ['#fef3c7', '#0f766e']
          : raceLineColorMode === 'brake'
            ? ['#fee2e2', '#b91c1c']
            : ['#2563eb', '#fef3c7', '#f59e0b']

    return {
      tooltip: { trigger: 'axis' },
      grid: { left: 10, right: 10, top: 10, bottom: 10 },
      xAxis: { type: 'value', show: false },
      yAxis: { type: 'value', show: false },
      visualMap: {
        min: minValue,
        max: maxValue,
        show: false,
        dimension: 2,
        inRange: {
          color: colorRange,
        },
      },
      series: [
        {
          type: 'line',
          data: coloredPoints,
          smooth: smoothLines,
          showSymbol: false,
          lineStyle: { width: 2 },
        },
        {
          type: 'scatter',
          data: throttlePoints,
          symbolSize: 4,
          itemStyle: {
            color: (params) => `rgba(15, 118, 110, ${Math.min(1, params.value[2] / 100)})`,
          },
        },
        {
          type: 'scatter',
          data: brakePoints,
          symbolSize: 4,
          itemStyle: {
            color: (params) => `rgba(185, 28, 28, ${Math.min(1, params.value[2] / 100)})`,
          },
        },
        ...(showPeaksValleys
          ? [
              {
                type: 'scatter',
                data: peakMarkers,
                symbolSize: 8,
                itemStyle: { color: '#f59e0b' },
                symbol: 'triangle',
              },
              {
                type: 'scatter',
                data: valleyMarkers,
                symbolSize: 8,
                itemStyle: { color: '#2563eb' },
                symbol: 'diamond',
              },
            ]
          : []),
      ],
    }
  }, [
    trackPoints,
    referenceSamples,
    compareSamples,
    buildDistanceSeries,
    resampleByDistance,
    decimate,
    smoothLines,
    raceLineColorMode,
    detectPeaksValleys,
    showPeaksValleys,
  ])

  const liveThrottlePct = livePayload ? Math.round((livePayload.throttle / 255) * 100) : 0
  const liveBrakePct = livePayload ? Math.round((livePayload.brake / 255) * 100) : 0
  const liveLapDisplay = livePayload
    ? `${livePayload.lapCount} / ${livePayload.lapsInRace}`
    : '—'
  const liveFuelPct = livePayload ? Math.round(livePayload.fuelPct) : 0
  const dbSizeLabel = useMemo(() => {
    if (!dbInfo?.sizeBytes) return '—'
    const sizes = ['B', 'KB', 'MB', 'GB']
    let size = dbInfo.sizeBytes
    let unit = 0
    while (size >= 1024 && unit < sizes.length - 1) {
      size /= 1024
      unit += 1
    }
    return `${size.toFixed(unit === 0 ? 0 : 1)} ${sizes[unit]}`
  }, [dbInfo])

  return (
    <div className="app">
      <header className="app-header">
        <div>
          <p className="eyebrow">GT7 Telemetry</p>
          <h1>Race data, captured and compared.</h1>
          <p className="lede">
            A native macOS cockpit for GT7 telemetry with live dashboards,
            lap comparisons, and detailed analysis.
          </p>
        </div>
        <div className="status-card">
          <div className="status-row">
            <span className="label">App Version</span>
            <span className="value">v{status.version}</span>
          </div>
          <div className="status-row">
            <span className="label">Listener</span>
            <span className={`value ${status.listenerRunning ? 'ok' : 'idle'}`}>
              {status.listenerRunning ? 'Running' : 'Stopped'}
            </span>
          </div>
          <div className="status-row">
            <span className="label">Last Packet</span>
            <span className="value">{lastPacketLabel}</span>
          </div>
          <div className="status-row">
            <span className="label">Database</span>
            <span className="value">{status.dbPath ?? 'Not set'}</span>
          </div>
          <div className="status-row">
            <span className="label">PS5 Target</span>
            <span className="value">{status.targetIp ?? 'Not set'}</span>
          </div>
          <div className="status-row">
            <span className="label">Session</span>
            <span className="value">{status.currentSessionId ?? 'None'}</span>
          </div>
          <div className="status-row">
            <span className="label">Bound Ports</span>
            <span className="value">
              {status.boundPorts.length ? status.boundPorts.join(', ') : 'None'}
            </span>
          </div>
          <div className="status-row">
            <span className="label">Heartbeat</span>
            <span className="value">
              {status.lastHeartbeatAt
                ? new Date(status.lastHeartbeatAt).toLocaleTimeString()
                : 'Not sent'}
            </span>
          </div>
          {status.lastListenerError ? (
            <p className="error">{status.lastListenerError}</p>
          ) : null}
          <div className="status-row">
            <span className="label">Packets Seen</span>
            <span className="value">{status.packetCount}</span>
          </div>
          {status.lastPacketMeta ? (
            <div className="meta">
              <div>
                <span className="label">Magic</span>
                <span className="value">
                  {status.lastPacketMeta.magic ?? '—'}
                </span>
              </div>
              <div>
                <span className="label">Packet ID</span>
                <span className="value">
                  {status.lastPacketMeta.packetId ?? '—'}
                </span>
              </div>
              <div>
                <span className="label">Payload</span>
                <span className="value">{status.lastPacketMeta.payloadLen} B</span>
              </div>
            </div>
          ) : null}
          {status.lastSample ? (
            <div className="meta">
              <div>
                <span className="label">Packet ID</span>
                <span className="value">{status.lastSample.packetId}</span>
              </div>
              <div>
                <span className="label">Speed</span>
                <span className="value">{status.lastSample.speedKmh.toFixed(1)} km/h</span>
              </div>
              <div>
                <span className="label">RPM</span>
                <span className="value">{status.lastSample.engineRpm.toFixed(0)}</span>
              </div>
              <div>
                <span className="label">Throttle</span>
                <span className="value">
                  {Math.round((status.lastSample.throttle / 255) * 100)}%
                </span>
              </div>
              <div>
                <span className="label">Brake</span>
                <span className="value">
                  {Math.round((status.lastSample.brake / 255) * 100)}%
                </span>
              </div>
              <div>
                <span className="label">Gear</span>
                <span className="value">{status.lastSample.gear}</span>
              </div>
              <div>
                <span className="label">Lap</span>
                <span className="value">{status.lastSample.lapCount}</span>
              </div>
            </div>
          ) : null}
          {error ? <p className="error">{error}</p> : null}
          <div className="actions">
            <button type="button" onClick={toggleListener} disabled={loading}>
              {status.listenerRunning ? 'Stop Listener' : 'Start Listener'}
            </button>
            <button type="button" onClick={initDatabase} disabled={loading}>
              Initialize DB
            </button>
            <button type="button" className="ghost" onClick={refreshStatus} disabled={loading}>
              Refresh
            </button>
          </div>
          <div className="ip-form">
            <label htmlFor="ps5-ip">PS5 IP Address</label>
            <div>
              <input
                id="ps5-ip"
                type="text"
                placeholder="192.168.0.10"
                value={targetIp}
                onChange={(event) => setTargetIp(event.target.value)}
              />
              <button type="button" className="ghost" onClick={saveTargetIp} disabled={loading}>
                Save
              </button>
            </div>
          </div>
          <div className="ip-form">
            <label htmlFor="session-picker">Session Picker</label>
            <div>
              <select
                id="session-picker"
                value={currentSessionId ?? ''}
                onChange={(event) => selectSession(event.target.value)}
              >
                <option value="">Select a session</option>
                {sessions.map((session) => (
                  <option key={session.id} value={String(session.id)}>
                    {new Date(session.startedAt).toLocaleString()} · #{session.id}
                  </option>
                ))}
              </select>
            </div>
          </div>
          <div className="session-summary">
            <div>
              <span className="label">Laps</span>
              <span className="value">
                {sessions.find((session) => session.id === currentSessionId)?.lapCount ?? '—'}
              </span>
            </div>
            <div>
              <span className="label">Best Lap</span>
              <span className="value">
                {formatLapTime(
                  sessions.find((session) => session.id === currentSessionId)?.bestLapMs,
                )}
              </span>
            </div>
            <div>
              <span className="label">Duration</span>
              <span className="value">
                {formatDuration(
                  sessions.find((session) => session.id === currentSessionId)?.durationMs,
                )}
              </span>
            </div>
          </div>
          <div className="ip-form">
            <label htmlFor="reference-lap">Reference Lap</label>
            <div>
              <select
                id="reference-lap"
                value={referenceLapId}
                onChange={(event) => {
                  const next = event.target.value
                  setReferenceLapId(next)
                  saveSessionPreferences(next, compareLapId)
                }}
              >
                <option value="">Select a lap</option>
                {laps.map((lap) => (
                  <option key={lap.id} value={String(lap.id)}>
                    Lap {lap.lapIndex} · {formatLapTime(lap.lapTimeMs)}
                  </option>
                ))}
              </select>
            </div>
          </div>
          <div className="ip-form">
            <label htmlFor="compare-lap">Compare Lap</label>
            <div>
              <select
                id="compare-lap"
                value={compareLapId}
                onChange={(event) => {
                  const next = event.target.value
                  setCompareLapId(next)
                  saveSessionPreferences(referenceLapId, next)
                }}
              >
                <option value="">Select a lap</option>
                {laps.map((lap) => (
                  <option key={lap.id} value={String(lap.id)}>
                    Lap {lap.lapIndex} · {formatLapTime(lap.lapTimeMs)} {lap.isLastLap ? '(Last)' : ''}
                  </option>
                ))}
              </select>
              <div className="quick-selects">
                <button type="button" className="ghost" onClick={useLastLap} disabled={!lastLapId}>
                  Use Last Lap
                </button>
                <button type="button" className="ghost" onClick={useMedianLap} disabled={!medianLapId}>
                  Use Median Lap
                </button>
              </div>
            </div>
          </div>
          <div className="chart-controls">
            <label>
              <input
                type="checkbox"
                checked={smoothLines}
                onChange={(event) => setSmoothLines(event.target.checked)}
              />
              Smooth lines
            </label>
            <label>
              <input
                type="checkbox"
                checked={showLegends}
                onChange={(event) => setShowLegends(event.target.checked)}
              />
              Show legends
            </label>
            <label>
              <input
                type="checkbox"
                checked={showPeaksValleys}
                onChange={(event) => setShowPeaksValleys(event.target.checked)}
              />
              Show peaks/valleys
            </label>
            <label>
              Peak preset
              <select
                value={peakPreset}
                onChange={(event) => applyPeakPreset(event.target.value)}
              >
                <option value="aggressive">Aggressive</option>
                <option value="balanced">Balanced</option>
                <option value="smooth">Smooth</option>
              </select>
            </label>
            <label>
              Peak threshold
              <input
                type="number"
                min="1"
                max="20"
                value={peakThreshold}
                onChange={(event) => setPeakThreshold(Number(event.target.value))}
              />
            </label>
            <label>
              Peak spacing
              <input
                type="number"
                min="2"
                max="20"
                value={peakSpacing}
                onChange={(event) => setPeakSpacing(Number(event.target.value))}
              />
            </label>
            <label>
              Race line color
              <select
                value={raceLineColorMode}
                onChange={(event) => setRaceLineColorMode(event.target.value)}
              >
                <option value="delta">Delta speed</option>
                <option value="throttle">Throttle</option>
                <option value="brake">Brake</option>
                <option value="peaks">Peaks/Valleys</option>
              </select>
            </label>
            <label>
              Export samples/lap
              <input
                type="number"
                min="100"
                max="5000"
                value={exportLimit}
                onChange={(event) => setExportLimit(Number(event.target.value))}
              />
            </label>
            <div className="control-group">
              <label>
                <input
                  type="checkbox"
                  checked={showVariance}
                  onChange={(event) => setShowVariance(event.target.checked)}
                />
                Show speed variance
              </label>
              <select
                value={varianceLapCount}
                onChange={(event) => setVarianceLapCount(Number(event.target.value))}
                disabled={!showVariance}
              >
                <option value={3}>Top 3 laps</option>
                <option value={5}>Top 5 laps</option>
                <option value={10}>Top 10 laps</option>
              </select>
            </div>
            <div className="control-group">
              <label>Replay filter</label>
              <select
                value={replayFilter}
                onChange={(event) => setReplayFilter(event.target.value)}
              >
                <option value="all">All Laps</option>
                <option value="replays">Replays Only</option>
                <option value="live">Live Only</option>
              </select>
            </div>
            <button type="button" onClick={exportSession} disabled={loading || !currentSessionId}>
              Export Session JSON
            </button>
            <label>
              Import session
              <input
                type="file"
                accept="application/json"
                onChange={(event) => previewImport(event.target.files?.[0])}
              />
            </label>
            {importPreview ? (
              <div className="import-preview">
                <div>
                  <strong>Preview</strong>
                </div>
                <div>File: {importFile?.name ?? '—'}</div>
                <div>Laps: {importPreview.lapCount}</div>
                <div>
                  Prefs: {importPreview.preferences?.raceLineColorMode ?? 'default'} · peaks
                  {importPreview.preferences?.showPeaks ? ' on' : ' off'}
                </div>
                <button type="button" onClick={confirmImport} disabled={loading}>
                  Confirm Import
                </button>
                <button
                  type="button"
                  className="ghost"
                  onClick={() => {
                    setImportPreview(null)
                    setImportFile(null)
                  }}
                  disabled={loading}
                >
                  Cancel
                </button>
              </div>
            ) : null}
            {importStatus ? <span className="import-status">{importStatus}</span> : null}
          </div>
        </div>
      </header>

      <section className="grid">
        <article>
          <h2>Live Dashboard</h2>
          <p>
            Track speed, throttle, brake, and RPM in real time with responsive
            charts optimized for quick glances.
          </p>
          <div className="live-panel">
            <div className="live-main">
              <div>
                <span className="label">Speed</span>
                <span className="value">{livePayload ? livePayload.speedKmh.toFixed(0) : '—'} km/h</span>
              </div>
              <div>
                <span className="label">RPM</span>
                <span className="value">{livePayload ? livePayload.engineRpm.toFixed(0) : '—'}</span>
              </div>
              <div>
                <span className="label">Gear</span>
                <span className="value">{livePayload ? livePayload.gear : '—'}</span>
              </div>
              <div>
                <span className="label">Lap</span>
                <span className="value">{liveLapDisplay}</span>
              </div>
            </div>
            <div className="live-bars">
              <div>
                <span className="label">Throttle</span>
                <span className="value">{livePayload ? `${liveThrottlePct}%` : '—'}</span>
                <div className="bar">
                  <span style={{ width: `${liveThrottlePct}%` }} />
                </div>
              </div>
              <div>
                <span className="label">Brake</span>
                <span className="value">{livePayload ? `${liveBrakePct}%` : '—'}</span>
                <div className="bar is-brake">
                  <span style={{ width: `${liveBrakePct}%` }} />
                </div>
              </div>
              <div>
                <span className="label">Fuel</span>
                <span className="value">{livePayload ? `${liveFuelPct}%` : '—'}</span>
                <div className="bar is-fuel">
                  <span style={{ width: `${liveFuelPct}%` }} />
                </div>
              </div>
            </div>
            <div className="live-temps">
              <div>
                <span className="label">Water</span>
                <span className="value">{livePayload ? `${livePayload.waterTemp.toFixed(0)}°` : '—'}</span>
              </div>
              <div>
                <span className="label">Oil</span>
                <span className="value">{livePayload ? `${livePayload.oilTemp.toFixed(0)}°` : '—'}</span>
              </div>
              <div>
                <span className="label">Tires</span>
                <div className="tire-grid">
                  <span>{livePayload ? `${livePayload.tireTempFl.toFixed(0)}°` : '—'}</span>
                  <span>{livePayload ? `${livePayload.tireTempFr.toFixed(0)}°` : '—'}</span>
                  <span>{livePayload ? `${livePayload.tireTempRl.toFixed(0)}°` : '—'}</span>
                  <span>{livePayload ? `${livePayload.tireTempRr.toFixed(0)}°` : '—'}</span>
                </div>
              </div>
            </div>
            <div className="live-meta">
              <div>
                <span className="label">Lap Time</span>
                <span className="value">
                  {livePayload ? formatLapTime(livePayload.currentLapTimeMs) : '—'}
                </span>
              </div>
              <div>
                <span className="label">Last</span>
                <span className="value">
                  {livePayload ? formatLapTime(livePayload.lastLapTimeMs) : '—'}
                </span>
              </div>
              <div>
                <span className="label">Best</span>
                <span className="value">
                  {livePayload ? formatLapTime(livePayload.bestLapTimeMs) : '—'}
                </span>
              </div>
              <div className="flag-row">
                <span className={livePayload?.asmActive ? 'flag on' : 'flag'}>ASM</span>
                <span className={livePayload?.tcsActive ? 'flag on' : 'flag'}>TCS</span>
                <span className={livePayload?.revLimiterActive ? 'flag on' : 'flag'}>REV</span>
              </div>
            </div>
          </div>
        </article>
        <article>
          <h2>Lap Intelligence</h2>
          <p>
            Compare best, median, and reference laps with delta charts, speed
            variance, and peaks/valleys analysis.
          </p>
          <div className="chart-card">
            <ReactECharts option={comparisonOption} style={{ height: '100%', width: '100%' }} />
          </div>
          <div className="chart-card">
            <ReactECharts option={throttleBrakeOption} style={{ height: '100%', width: '100%' }} />
          </div>
          <div className="chart-card">
            <ReactECharts option={rpmOption} style={{ height: '100%', width: '100%' }} />
          </div>
          <div className="chart-card tall">
            <ReactECharts option={splitViewOption} style={{ height: '100%', width: '100%' }} />
          </div>
        </article>
        <article>
          <h2>Race Line</h2>
          <p>
            Visualize racing lines with throttle, brake, and coasting overlays
            to understand pace across corners.
          </p>
          <div className="chart-card">
            <ReactECharts option={trackOption} style={{ height: '100%', width: '100%' }} />
          </div>
        </article>
        <article>
          <h2>Fuel Strategy</h2>
          <p>
            Estimate fuel map impact and plan stints with lap time and distance
            projections.
          </p>
          {fuelAnalysis ? (
            <div className="fuel-panel">
              <div className="fuel-summary">
                <div>
                  <span className="label">Current Fuel</span>
                  <span className="value">{fuelAnalysis.currentFuel.toFixed(1)} L</span>
                </div>
                <div>
                  <span className="label">Fuel Capacity</span>
                  <span className="value">{fuelAnalysis.fuelCapacity.toFixed(1)} L</span>
                </div>
                <div>
                  <span className="label">Avg Consumption</span>
                  <span className="value">{fuelAnalysis.avgConsumptionPerLap.toFixed(2)} L/lap</span>
                </div>
                <div>
                  <span className="label">Projected Laps</span>
                  <span className="value">{fuelAnalysis.projectedLapsRemaining.toFixed(1)}</span>
                </div>
              </div>
              <div className="fuel-table-wrapper">
                <table className="fuel-table">
                  <thead>
                    <tr>
                      <th>Lap</th>
                      <th>Fuel Start</th>
                      <th>Fuel End</th>
                      <th>Consumed</th>
                    </tr>
                  </thead>
                  <tbody>
                    {fuelAnalysis.laps.map((lap) => (
                      <tr key={lap.lapId}>
                        <td>{lap.lapIndex}</td>
                        <td>{lap.fuelStart.toFixed(2)} L</td>
                        <td>{lap.fuelEnd.toFixed(2)} L</td>
                        <td>{lap.consumed.toFixed(2)} L</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          ) : (
            <div className="chart-card" aria-hidden="true">
              <div className="placeholder">Start a session to see fuel analysis.</div>
            </div>
          )}
        </article>
        <article>
          <h2>Lap Details</h2>
          <p>
            Comprehensive lap metrics with sorting and quick selection.
          </p>
          {detailedLaps.length > 0 ? (
            <div className="lap-table-wrapper">
              <table className="lap-table">
                <thead>
                  <tr>
                    <th>Lap</th>
                    <th>Lap Time</th>
                    <th>Delta</th>
                    <th>Max Speed</th>
                    <th>Avg Speed</th>
                    <th>Throttle %</th>
                    <th>Brake %</th>
                    <th>Fuel</th>
                    <th>Body Ht</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {detailedLaps
                    .filter((lap) => {
                      if (replayFilter === 'replays') return lap.isReplay
                      if (replayFilter === 'live') return !lap.isReplay
                      return true
                    })
                    .map((lap) => (
                      <tr key={lap.id} className={lap.isLastLap ? 'is-last' : ''}>
                        <td>
                          {lap.lapIndex}
                          {lap.isLastLap && <span className="last-badge">Last</span>}
                          {lap.isReplay && <span className="replay-badge">Replay</span>}
                        </td>
                        <td>{formatLapTime(lap.lapTimeMs)}</td>
                        <td>{lap.deltaToBestMs ? `+${(lap.deltaToBestMs / 1000).toFixed(3)}s` : '—'}</td>
                        <td>{lap.maxSpeedKmh.toFixed(0)}</td>
                        <td>{lap.avgSpeedKmh.toFixed(0)}</td>
                        <td>{lap.throttlePct.toFixed(0)}%</td>
                        <td>{lap.brakePct.toFixed(0)}%</td>
                        <td>{lap.fuelConsumed.toFixed(2)} L</td>
                        <td>{lap.minBodyHeight.toFixed(3)}</td>
                        <td className="actions">
                          <button
                            type="button"
                            className="ghost small"
                            onClick={() => {
                              setReferenceLapId(String(lap.id))
                              saveSessionPreferences(String(lap.id), compareLapId)
                            }}
                          >
                            Ref
                          </button>
                          <button
                            type="button"
                            className="ghost small"
                            onClick={() => {
                              setCompareLapId(String(lap.id))
                              saveSessionPreferences(referenceLapId, String(lap.id))
                            }}
                          >
                            Cmp
                          </button>
                        </td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="chart-card" aria-hidden="true">
              <div className="placeholder">Start a session to see lap details.</div>
            </div>
          )}
        </article>
        <article>
          <h2>Database Management</h2>
          <p>Check storage health, clean up, or reset telemetry data.</p>
          <div className="db-panel">
            <div>
              <span className="label">Path</span>
              <span className="value">{dbInfo?.path ?? status.dbPath ?? '—'}</span>
            </div>
            <div>
              <span className="label">Size</span>
              <span className="value">{dbSizeLabel}</span>
            </div>
            <div>
              <span className="label">Sessions</span>
              <span className="value">{dbInfo?.sessions ?? '—'}</span>
            </div>
            <div>
              <span className="label">Laps</span>
              <span className="value">{dbInfo?.laps ?? '—'}</span>
            </div>
            <div>
              <span className="label">Samples</span>
              <span className="value">{dbInfo?.samples ?? '—'}</span>
            </div>
            <div>
              <span className="label">Last Sample</span>
              <span className="value">
                {dbInfo?.lastSampleTs ? new Date(dbInfo.lastSampleTs).toLocaleString() : '—'}
              </span>
            </div>
          </div>
          <div className="actions">
            <button type="button" onClick={initDatabase} disabled={loading}>
              Initialize DB
            </button>
            <button type="button" className="ghost" onClick={vacuumDatabase} disabled={loading}>
              Vacuum DB
            </button>
            <button type="button" className="ghost" onClick={resetDatabase} disabled={loading}>
              Reset DB
            </button>
          </div>
          <div className="db-actions">
            <div className="db-row">
              <span className="label">Delete Session</span>
              <button
                type="button"
                className="ghost"
                onClick={deleteSession}
                disabled={loading || !currentSessionId}
              >
                Delete Current Session
              </button>
            </div>
            <div className="db-row">
              <label className="label" htmlFor="delete-lap">
                Delete Lap
              </label>
              <div className="db-select">
                <select
                  id="delete-lap"
                  value={deleteLapId}
                  onChange={(event) => setDeleteLapId(event.target.value)}
                >
                  <option value="">Select a lap</option>
                  {laps.map((lap) => (
                    <option key={lap.id} value={String(lap.id)}>
                      Lap {lap.lapIndex} · {formatLapTime(lap.lapTimeMs)}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  className="ghost"
                  onClick={deleteLap}
                  disabled={loading || !deleteLapId}
                >
                  Delete Lap
                </button>
              </div>
            </div>
          </div>
        </article>
      </section>

    </div>
  )
}

export default App
