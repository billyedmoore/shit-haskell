{-# OPTIONS_GHC -Wno-deprecations #-}
import System.Mem
import System.Posix.Signals
import System.Posix.Unistd

main :: IO ()
main = do
  -- A blocked alarm stays pending (it does not end the program).
  blockSignals (addSignal realTimeAlarm emptySignalSet)
  _ <- scheduleAlarm 1
  _ <- sleep 2
  pend <- getPendingSignals
  print (inSignalSet realTimeAlarm pend)
  -- Collections on request.
  performGC
  performMinorGC
  performMajorGC
  putStrLn "collected"
