-- An implicit parameter of type CallStack with no binding is the empty stack, whatever its name
-- (basement's `error s = raise# (errorCallWithCallStackException s ?callstack)`).
{-# LANGUAGE ImplicitParams #-}
import GHC.Stack (CallStack, getCallStack)

frames :: Int
frames = length (getCallStack ?callstack)

main :: IO ()
main = print frames
