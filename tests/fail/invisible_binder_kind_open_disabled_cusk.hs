{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,NoCUSKs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family T @k (a :: k)
main :: IO ()
main = pure ()
