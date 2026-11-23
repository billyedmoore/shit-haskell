{-# LANGUAGE PolyKinds, TypeApplications, TypeFamilies #-}
import Data.Proxy (Proxy)

data family T (a :: k)
data instance T @k a = T (Proxy (a :: k))

main :: IO ()
main = pure ()
