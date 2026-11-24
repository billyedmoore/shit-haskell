{-# LANGUAGE ExistentialQuantification, PolyKinds, NoScopedTypeVariables #-}
import Data.Proxy
data T (a :: k) = forall (b :: k). T (Proxy a) (Proxy b)
main :: IO ()
main = pure ()
