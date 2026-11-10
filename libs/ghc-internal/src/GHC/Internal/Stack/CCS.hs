{-# LANGUAGE Trustworthy #-}
{-# LANGUAGE NoImplicitPrelude #-}

-----------------------------------------------------------------------------
-- |
-- Module      :  GHC.Internal.Stack.CCS
-- Stability   :  internal
--
-- Access to GHC's call-stack simulation.
--
-- hsc does not support profiling, so no cost-centre stack is ever recorded.
-- This is the stand-in for GHC's CCS.hsc: the same interface, which behaves
-- as a program compiled without @-prof@ does (every stack is empty, every
-- pointer is null).
-----------------------------------------------------------------------------

module GHC.Internal.Stack.CCS (
    -- * Call stacks
    currentCallStack,
    whoCreated,

    -- * Internals
    CostCentreStack,
    CostCentre,
    getCurrentCCS,
    getCCSOf,
    clearCCS,
    ccsCC,
    ccsParent,
    ccLabel,
    ccModule,
    ccSrcSpan,
    ccsToStrings,
    renderStack,
  ) where

import GHC.Internal.Base
import GHC.Internal.Ptr
import GHC.Internal.Foreign.C.String (CString)
import GHC.Internal.List (concatMap, reverse)

-- | A cost-centre stack from GHC's cost-center profiler.
data CostCentreStack

-- | A cost-centre from GHC's cost-center profiler.
data CostCentre

-- | Returns the current 'CostCentreStack' (always null: profiling is not supported).
getCurrentCCS :: dummy -> IO (Ptr CostCentreStack)
getCurrentCCS _ = return nullPtr

-- | Get the 'CostCentreStack' associated with the given value (always null).
getCCSOf :: a -> IO (Ptr CostCentreStack)
getCCSOf _ = return nullPtr

-- | Run a computation with an empty cost-center stack.
clearCCS :: IO a -> IO a
clearCCS m = m

-- | Get the 'CostCentre' at the head of a 'CostCentreStack'.
ccsCC :: Ptr CostCentreStack -> IO (Ptr CostCentre)
ccsCC _ = return nullPtr

-- | Get the tail of a 'CostCentreStack'.
ccsParent :: Ptr CostCentreStack -> IO (Ptr CostCentreStack)
ccsParent _ = return nullPtr

-- | Get the label of a 'CostCentre'.
ccLabel :: Ptr CostCentre -> IO CString
ccLabel _ = return nullPtr

-- | Get the module of a 'CostCentre'.
ccModule :: Ptr CostCentre -> IO CString
ccModule _ = return nullPtr

-- | Get the source span of a 'CostCentre'.
ccSrcSpan :: Ptr CostCentre -> IO CString
ccSrcSpan _ = return nullPtr

-- | The strings of a cost-centre stack (none, without profiling).
ccsToStrings :: Ptr CostCentreStack -> IO [String]
ccsToStrings _ = return []

-- | Returns a @[String]@ representing the current call stack (empty without profiling).
currentCallStack :: IO [String]
currentCallStack = return []

-- | The call-stack strings of the cost centre that created a value (empty without profiling).
whoCreated :: a -> IO [String]
whoCreated _ = return []

-- | Render a list of call-stack strings the way GHC does.
renderStack :: [String] -> String
renderStack strs =
  "CallStack (from -prof):" ++ concatMap ("\n  " ++) (reverse strs)
