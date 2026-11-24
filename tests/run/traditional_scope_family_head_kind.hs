{-# LANGUAGE ExistentialQuantification, TypeFamilies, PolyKinds #-}
import Data.Proxy
data family T (a :: k)
data instance T (a :: k) = forall (b :: k). T (Proxy a) (Proxy b)
main :: IO ()
main = pure ()
