module System.Exit (ExitCode(..), exitWith, exitFailure, exitSuccess) where

import Base

data ExitCode = ExitSuccess | ExitFailure Int

instance Eq ExitCode where
  ExitSuccess == ExitSuccess = True
  ExitFailure a == ExitFailure b = a == b
  _ == _ = False

instance Show ExitCode where
  showsPrec _ ExitSuccess = showString "ExitSuccess"
  showsPrec d (ExitFailure n) = showParen (d > 10) (showString "ExitFailure " . showsPrec 11 n)

exitWith :: ExitCode -> IO a
exitWith ExitSuccess = IO (\w -> case primExitWith 0 w of IORes _ -> primError "unreachable")
exitWith (ExitFailure n) = IO (\w -> case primExitWith n w of IORes _ -> primError "unreachable")

exitSuccess :: IO a
exitSuccess = exitWith ExitSuccess

exitFailure :: IO a
exitFailure = exitWith (ExitFailure 1)
