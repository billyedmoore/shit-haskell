module Cls (Throw (..), Box (..)) where
class Monad m => Throw m where
  throwIt :: String -> m a
  other :: m ()
  other = return ()
data Box = Box { unBox :: Int } | Empty
