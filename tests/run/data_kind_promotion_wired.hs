{-# LANGUAGE NoDataKinds #-}

import Data.Proxy
import GHC.Exts (RuntimeRep(..), Levity(..), VecCount(..), VecElem(..))

intRep :: Proxy 'IntRep
intRep = Proxy

lifted :: Proxy 'Lifted
lifted = Proxy

vec2 :: Proxy 'Vec2
vec2 = Proxy

elemRep :: Proxy 'Int8ElemRep
elemRep = Proxy

main :: IO ()
main = pure ()
