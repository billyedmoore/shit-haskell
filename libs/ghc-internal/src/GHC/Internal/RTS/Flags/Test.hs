{-# LANGUAGE NoImplicitPrelude #-}
-- Stand-in for the hsc2hs module Test.hsc, which reads the RTS's flag block. A program runs without
-- `+RTS -l`, so user event tracing is off, which is what this answers.
module GHC.Internal.RTS.Flags.Test
  ( getUserEventTracingEnabled
  )
where

import GHC.Internal.Base

getUserEventTracingEnabled :: IO Bool
getUserEventTracingEnabled = pure False
