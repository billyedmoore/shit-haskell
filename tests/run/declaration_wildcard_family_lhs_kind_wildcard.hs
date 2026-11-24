{-# LANGUAGE TypeFamilies, PolyKinds, TypeApplications #-}
import Data.Proxy (Proxy)

type family T (a :: k)
type instance T @_ a = Bool

main :: IO ()
main = pure ()
