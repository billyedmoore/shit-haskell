{-# LANGUAGE GADTs, PolyKinds #-}
import Data.Proxy (Proxy(..))

data T where
  T :: forall k (a :: k). Proxy a -> T

main :: IO ()
main = case T (Proxy :: Proxy Bool) of T _ -> putStrLn "ok"
