{-# LANGUAGE Trustworthy #-}

-----------------------------------------------------------------------------
-- |
-- Module      :  GHC.Internal.System.Environment.ExecutablePath
-- Copyright   :  (c) The University of Glasgow 2001
-- License     :  BSD-style (see the file libraries/base/LICENSE)
--
-- Maintainer  :  libraries@haskell.org
-- Stability   :  provisional
-- Portability :  portable
--
-- Function to retrieve the absolute filepath of the current executable.
--
-- hsc: GHC's source is ExecutablePath.hsc (hsc2hs); this is its Linux branch, preprocessed by hand.
--
-- @since base-4.6.0.0
-----------------------------------------------------------------------------

module GHC.Internal.System.Environment.ExecutablePath
  ( getExecutablePath
  , executablePath
  ) where

import GHC.Internal.Base
import GHC.Internal.IO (FilePath)
import GHC.Internal.Real
import GHC.Internal.Data.Functor
import GHC.Internal.Data.List (isSuffixOf)
import GHC.Internal.Foreign.C.Types
import GHC.Internal.Foreign.C.Error
import GHC.Internal.Foreign.C.String
import GHC.Internal.Foreign.Marshal.Array
import GHC.Internal.System.Posix.Internals

-- | Returns the absolute pathname of the current executable,
-- or @argv[0]@ if the operating system does not provide a reliable
-- way query the current executable.
--
-- Note that for scripts and interactive sessions, this is the path to
-- the interpreter (e.g. ghci.)
--
-- Since base 4.11.0.0, 'getExecutablePath' resolves symlinks on Windows.
-- If an executable is launched through a symlink, 'getExecutablePath'
-- returns the absolute path of the original executable.
--
-- If the executable has been deleted, behaviour is ill-defined and
-- varies by operating system.  See 'executablePath' for a more
-- reliable way to query the current executable.
--
-- @since base-4.6.0.0
getExecutablePath :: IO FilePath

-- | Get an action to query the absolute pathname of the current executable,
-- if supported by the operating system.
--
-- @since base-4.17.0.0
executablePath :: Maybe (IO (Maybe FilePath))

foreign import ccall unsafe "readlink"
    c_readlink :: CString -> CString -> CSize -> IO CInt

readSymbolicLink :: FilePath -> IO FilePath
readSymbolicLink file =
    allocaArray0 4096 $ \buf ->
        withFilePath file $ \s -> do
            len <- throwErrnoPathIfMinus1 "readSymbolicLink" file $
                   c_readlink s buf 4096
            peekFilePathLen (buf,fromIntegral len)

getExecutablePath = readSymbolicLink $ "/proc/self/exe"

executablePath = Just (check <$> getExecutablePath) where
  check s | "(deleted)" `isSuffixOf` s = Nothing
          | otherwise = Just s
