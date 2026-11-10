{-# LANGUAGE NoImplicitPrelude #-}

-- hsc stand-in for GHC's ExecutionStack/Internal.hsc. The real module unwinds the C stack through
-- libdw. hsc runs programs in its own interpreter, which has no C stack to walk, so no stack trace is
-- ever collected: this is the behaviour of the real module on a host without libdw. The types and the
-- rendering of frames are the same as the real module's.
module GHC.Internal.ExecutionStack.Internal (
  -- * Internal
    Location (..)
  , showLocation
  , SrcLoc (..)
  , StackTrace
  , stackFrames
  , stackDepth
  , collectStackTrace
  , showStackFrames
  , invalidateDebugCache
  ) where

import GHC.Internal.Base
import GHC.Internal.Data.Maybe
import GHC.Internal.Show (ShowS, showString, showChar, shows)

data SrcLoc = SrcLoc { sourceFile   :: String
                     , sourceLine   :: Int
                     , sourceColumn :: Int
                     }

data Location = Location { objectName   :: String
                         , functionName :: String
                         , srcLoc       :: Maybe SrcLoc
                         }

-- | A captured stack. None is ever captured here.
data StackTrace

stackDepth :: StackTrace -> Int
stackDepth _ = 0

stackFrames :: StackTrace -> Maybe [Location]
stackFrames _ = Nothing

collectStackTrace :: IO (Maybe StackTrace)
collectStackTrace = return Nothing

invalidateDebugCache :: IO ()
invalidateDebugCache = return ()

showStackFrames :: [Location] -> ShowS
showStackFrames frames =
    showString "Stack trace:\n"
    . foldr (.) id (map showFrame frames)
  where
    showFrame loc =
      showString "    " . showLocation loc . showChar '\n'

-- | Render a 'Location' as a string
showLocation :: Location -> ShowS
showLocation loc =
        showString (functionName loc)
      . maybe id showSrcLoc (srcLoc loc)
      . showString " in "
      . showString (objectName loc)
  where
    showSrcLoc :: SrcLoc -> ShowS
    showSrcLoc sloc =
        showString " ("
      . showString (sourceFile sloc)
      . showString ":"
      . shows (sourceLine sloc)
      . showString "."
      . shows (sourceColumn sloc)
      . showString ")"
