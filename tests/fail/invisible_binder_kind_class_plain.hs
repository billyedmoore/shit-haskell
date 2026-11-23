{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,CUSKs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
class C @k (a :: k)
main :: IO ()
main = pure ()
