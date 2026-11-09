module Debug.Trace (trace, traceShow, traceShowId, traceM, traceShowM, traceId) where

import Base

foreign import prim "traceErr" primTraceErr :: String -> a -> a

trace :: String -> a -> a
trace s x = primTraceErr s x

traceId :: String -> String
traceId s = trace s s

traceShow :: Show a => a -> b -> b
traceShow v x = trace (show v) x

traceShowId :: Show a => a -> a
traceShowId v = trace (show v) v

traceM :: Applicative f => String -> f ()
traceM s = trace s (pure ())

traceShowM :: (Show a, Applicative f) => a -> f ()
traceShowM v = traceM (show v)
