{-# LANGUAGE ExistentialQuantification, PolyKinds #-}
import Data.Proxy
data T = forall (a :: k). T (Proxy a)
main :: IO ()
main = pure ()
