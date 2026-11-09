module System.Mem (performGC, performMajorGC, performMinorGC) where

import Base

-- Garbage collection is reference counting, so there is nothing to force.
performGC :: IO ()
performGC = return ()

performMajorGC :: IO ()
performMajorGC = return ()

performMinorGC :: IO ()
performMinorGC = return ()
