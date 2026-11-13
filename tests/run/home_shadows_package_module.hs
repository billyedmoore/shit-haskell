-- A home module named like a hidden module of a package (time-compat's
-- Data.Time.Calendar.Private beside time's): the package's modules use their own.
module Main where

import Data.Time (fromGregorian, addDays, showGregorian)
import Data.Time.Calendar.Private (pad2)

main :: IO ()
main = do
  let d = fromGregorian 2024 2 28
  putStrLn (showGregorian (addDays 2 d))
  putStrLn (pad2 d)
  print (fromGregorian 2023 12 31 < d)
