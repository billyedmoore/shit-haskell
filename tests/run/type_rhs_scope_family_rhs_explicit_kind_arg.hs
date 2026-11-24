{-# LANGUAGE TypeFamilies, TypeApplications, PolyKinds #-}
import Data.Proxy (Proxy)

type family T (a :: k)
type instance T @k a = Proxy (a :: k)

main :: IO ()
main = pure ()
