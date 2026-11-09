module System.IO.Error (IOError, ioError, userError, catchIOError, isDoesNotExistError, isUserError, ioeGetErrorString, mkIOError, tryIOError) where

import Base
import Control.Exception

catchIOError :: IO a -> (IOError -> IO a) -> IO a
catchIOError = catch

tryIOError :: IO a -> IO (Either IOError a)
tryIOError = try

isDoesNotExistError :: IOError -> Bool
isDoesNotExistError e = contains "does not exist" (show e)

isUserError :: IOError -> Bool
isUserError e = contains "user error" (show e)

ioeGetErrorString :: IOError -> String
ioeGetErrorString e = show e

mkIOError :: a -> String -> Maybe b -> Maybe String -> IOError
mkIOError _ s _ _ = userError s

contains :: String -> String -> Bool
contains needle hay = any (\t -> take (length needle) t == needle) (tailsS hay)
  where
    tailsS [] = [[]]
    tailsS xs@(_ : rest) = xs : tailsS rest
