{-# LANGUAGE GADTs, PolyKinds #-}
import Data.Proxy
data T where
  T :: forall (a :: missing). Proxy a -> T
main :: IO ()
main = pure ()
